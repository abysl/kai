use agni_net::bridge;
use kai::ai::driver::{self, BridgeLink, Driver, Link, Mind, MindKind, Out, TICK};
use kai::net::node;
use std::io::BufRead;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

#[path = "kai_cli/soak.rs"]
mod soak;

struct Args {
    join: Option<String>,
    commands: Option<String>,
    log: Option<String>,
    deck: Option<String>,
    battlefield: Option<usize>,
    name: Option<String>,
    playmat: Option<String>,
    brain: Option<MindKind>,
    model: Option<String>,
    catalog: Option<String>,
    chat: Option<String>,
}

fn usage() -> ! {
    eprintln!(
        "kai-cli — a headless seat at a kai table over spirit\n\n\
         usage: kai-cli [--join <host id>] [--commands <file|fifo>] [--log <file>]\n\
                        [--deck <decklist file or deck link>] [--battlefield <n>] [--name <player>]\n\
                        [--playmat <image link|card:<battlefield name>>]\n\
                        [--ai] [--brain random|auto|nanogpt] [--model <nanogpt model id>]\n\
                        [--catalog <store dir with the card set>]\n\
                        [--chat <file the player writes you: lines into>]\n\n\
         The store is $SPIRIT_STORE (use a store the desktop kai is not holding).\n\
         Commands (one per line): help tables join <host> state view do <n> (or act <n>) move <card> <zone> [i]\n\
         decks [full] (saved decks in the catalogue store)\n\
         play <card> draw [rune] exhaust <card> trash <card> recycle <card> hide <card> [zone]\n\
         reveal <card> spawn <token|name> <zone> [might] playmat <link|card:name|felt>\n\
         counter <card|seat> <name> <delta> deck <file|url> battlefield <n> deal quit\n\n\
         --ai hands the seat to the nanogpt brain; --brain random or auto plays the seat without a\n\
         model, the way the desktop's free opponent does.\n\n\
         Modes: kai-cli only joins tables, so the mode is chosen at the table. After the roll for first\n\
         player the winner sees `switch to rules enforced` / `switch to free table` beside `go first`\n\
         (`do <n>` picks it when the winner is this seat); every deck must be dealt before `go first`.\n\
         Under rules enforced the table charges costs, refuses illegal moves with a reason (`refused: …`),\n\
         and asks questions as numbered actions (`prompt:` line, answer with `do <n>`); play <card>,\n\
         move <card> base and move <card> battlefield-N are the only free-form entries it accepts.\n\
         The turn player can propose `free table` and another seat confirms it; the game then goes on\n\
         without enforcement.\n\n\
         kai-cli soak [--games <n>] … plays self-play games between two saved decks in-process; see\n\
         `kai-cli soak --help`."
    );
    std::process::exit(2)
}

fn parse_args() -> Args {
    let mut args = Args {
        join: None,
        commands: None,
        log: None,
        deck: None,
        battlefield: None,
        name: None,
        playmat: None,
        brain: None,
        model: None,
        catalog: None,
        chat: None,
    };
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        let mut value = || it.next().unwrap_or_else(|| usage());
        match flag.as_str() {
            "--join" => args.join = Some(value()),
            "--commands" => args.commands = Some(value()),
            "--log" => args.log = Some(value()),
            "--deck" => args.deck = Some(value()),
            "--battlefield" => args.battlefield = value().parse().ok(),
            "--name" => args.name = Some(value()),
            "--playmat" => args.playmat = Some(value()),
            "--ai" => args.brain = args.brain.or(Some(MindKind::Llm)),
            "--brain" => args.brain = Some(MindKind::parse(&value()).unwrap_or_else(|| usage())),
            "--model" => args.model = Some(value()),
            "--catalog" => args.catalog = Some(value()),
            "--chat" => args.chat = Some(value()),
            _ => usage(),
        }
    }
    args
}

fn command_reader(path: Option<String>) -> mpsc::Receiver<String> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || match path {
        None => {
            for line in std::io::stdin().lock().lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    return;
                }
            }
        }
        Some(path) => {
            let file = loop {
                match std::fs::File::open(&path) {
                    Ok(file) => break file,
                    Err(_) => std::thread::sleep(Duration::from_millis(500)),
                }
            };
            let mut reader = std::io::BufReader::new(file);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line) {
                    Ok(0) => std::thread::sleep(Duration::from_millis(200)),
                    Ok(_) => {
                        if tx.send(line.trim_end().to_string()).is_err() {
                            return;
                        }
                    }
                    Err(_) => return,
                }
            }
        }
    });
    rx
}

fn wait_for_node(out: &mut Out) -> node::Node {
    node::start();
    let started = Instant::now();
    loop {
        if let Some(node) = node::get() {
            out.line(format!("node {} — {}", node.node_id, node::status()));
            return node;
        }
        if started.elapsed() > Duration::from_secs(60) {
            out.line(format!("spirit node did not start: {}", node::status()));
            std::process::exit(1);
        }
        std::thread::sleep(TICK);
    }
}

fn join(node: &node::Node, host: &str, name: &str, out: &mut Out) {
    let Some(addr) = bridge::host_addr(&node.mesh, host) else {
        out.line(format!(
            "no address for host {host} yet — is it in the mesh? try `tables`"
        ));
        return;
    };
    out.line(format!("joining {host}…"));
    kai::engine::modules::reset_fetches();
    node.spawn(bridge::run_join(
        node.endpoint.clone(),
        node.mesh.clone(),
        addr,
        host.to_string(),
        name.to_string(),
    ));
}

fn run_command(
    driver: &mut Driver,
    link: &mut dyn Link,
    node: &node::Node,
    name: &str,
    line: &str,
) -> bool {
    let words: Vec<&str> = line.split_whitespace().collect();
    let Some((&verb, rest)) = words.split_first() else {
        return true;
    };
    let out = &mut driver.out;
    match verb {
        "peers" => {
            for peer in spirit_node::peers::snapshot() {
                out.line(format!(
                    "peer {} {} · {} · {} · seen {}s ago · dial failures {}{}",
                    kai::net::defaults::label_of(&peer.id).unwrap_or(""),
                    peer.id,
                    peer.state.label(),
                    peer.discovery.label(),
                    peer.last_activity
                        .elapsed()
                        .map(|d| d.as_secs())
                        .unwrap_or(0),
                    peer.dial_failures,
                    peer.last_dial_error
                        .as_deref()
                        .map(|error| format!(" · {error}"))
                        .unwrap_or_default()
                ));
            }
            out.line(format!(
                "known {} peers, node {}",
                node.mesh.known_peers().len(),
                node::status()
            ));
            true
        }
        "seed" => {
            match rest.first() {
                Some(ticket) => match node.mesh.seed(ticket) {
                    Ok(id) => out.line(format!("seeded {id}")),
                    Err(error) => out.line(format!("seed failed: {error}")),
                },
                None => out.line("seed <ticket or node id>"),
            }
            true
        }
        "tables" => {
            let tables = node.mesh.open_tables();
            if tables.is_empty() {
                out.line("no open tables in the mesh yet");
            }
            for table in tables {
                out.line(format!("table {} — host {}", table.name, table.host));
            }
            true
        }
        "join" => {
            match rest.first() {
                Some(host) => join(node, host, name, out),
                None => out.line("join <host id>"),
            }
            true
        }
        _ => driver.command(link, line),
    }
}

fn main() {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    if raw.first().map(String::as_str) == Some("soak") {
        soak::main(&raw[1..]);
    }
    let args = parse_args();
    driver::set_catalog_dir(
        args.catalog
            .as_ref()
            .map(PathBuf::from)
            .or_else(kai::os::paths::store_dir),
    );
    let mut out = Out::open(args.log.as_deref());
    let name = args
        .name
        .clone()
        .or_else(|| std::env::var("USER").ok())
        .unwrap_or_else(|| "kai-cli".into());
    let node = wait_for_node(&mut out);
    let commands = command_reader(args.commands.clone());
    let mut driver = Driver::new(out)
        .with_playmat(args.playmat.clone())
        .with_chat(args.chat.as_ref().map(PathBuf::from));
    if let Some(path) = &args.deck {
        driver = driver.with_deck_file(path, args.battlefield);
    }
    if let Some(kind) = args.brain {
        let mind = match kind {
            MindKind::Llm => {
                let model = args
                    .model
                    .clone()
                    .unwrap_or_else(|| kai::ai::nanogpt::DEFAULT_MODEL.to_string());
                let notes = kai::os::paths::store_dir().map(|dir| dir.join("ai-notes.md"));
                driver::llm_mind(&model, notes, &mut driver.out)
            }
            MindKind::Random => Mind::Random,
            MindKind::Auto => Mind::Auto,
        };
        driver = driver.with_mind(mind);
    }
    let mut link = BridgeLink;
    let mut join_wanted = args.join.clone();
    loop {
        if let Some(host) = join_wanted.take() {
            if bridge::host_addr(&node.mesh, &host).is_some() {
                join(&node, &host, &name, &mut driver.out);
            } else {
                join_wanted = Some(host);
            }
        }
        driver.tick(&mut link);
        while let Ok(line) = commands.try_recv() {
            let line = line.trim().to_string();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            driver.out.line(format!("> {line}"));
            if !run_command(&mut driver, &mut link, &node, &name, &line) {
                return;
            }
        }
        std::thread::sleep(TICK);
    }
}
