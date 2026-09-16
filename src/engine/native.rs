use agni_engine_host::{load_engine, ENGINE_GAS_BUDGET};
use agni_sim::engine::{Engine, NativeEngine};

pub fn hosting_engine() -> Result<(Box<dyn Engine>, Option<String>), String> {
    Ok(session_engine())
}

pub fn session_engine() -> (Box<dyn Engine>, Option<String>) {
    let (module, note) = crate::engine::modules::engine_module();
    match module {
        Some(module) => match load_engine(&module.bytes, ENGINE_GAS_BUDGET) {
            Ok(engine) => (Box::new(engine), Some(note)),
            Err(error) => (
                Box::new(NativeEngine::new()),
                Some(format!(
                    "{note}; engine refused ({error}) — folding natively"
                )),
            ),
        },
        None => (Box::new(NativeEngine::new()), Some(note)),
    }
}
