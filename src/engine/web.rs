use agni_sim::engine::{
    AbiEngine, AbiPlugin, CallFault, Engine, ModuleCall, NativeEngine, PluginModule,
    ENGINE_GAS_BUDGET, PLUGIN_GAS_BUDGET,
};
use agni_sim::pins::hash_hex;
use agni_sim::wire::{decode_plugin_manifest, PluginManifest};
use send_wrapper::SendWrapper;
use std::cell::RefCell;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

struct LoadedEngine {
    module: js_sys::WebAssembly::Module,
    bytes: Vec<u8>,
    hash: [u8; 32],
    provenance: String,
}

thread_local! {
    static MODULE: RefCell<Option<LoadedEngine>> = const { RefCell::new(None) };
    static STATUS: RefCell<String> = const { RefCell::new(String::new()) };
}

pub fn boot() {
    crate::engine::modules::fetch_bundle();
    wasm_bindgen_futures::spawn_local(async {
        match fetch_bundled_module().await {
            Ok((module, bytes)) => {
                let hash = *blake3::hash(&bytes).as_bytes();
                let provenance = format!(
                    "engine bundled from ./engine.wasm @ {} — no store engine yet",
                    crate::engine::modules::short_hex(&hash_hex(&hash))
                );
                let installed = MODULE.with(|slot| {
                    let slot = &mut *slot.borrow_mut();
                    if slot.is_some() {
                        return false;
                    }
                    *slot = Some(LoadedEngine {
                        module,
                        bytes: bytes.clone(),
                        hash,
                        provenance: provenance.clone(),
                    });
                    true
                });
                if installed {
                    crate::net::node::serve_bytes(bytes);
                    STATUS.with(|status| *status.borrow_mut() = provenance);
                }
            }
            Err(error) => {
                let line = format!("engine.wasm unavailable: {error}");
                bevy::log::warn!("{line}");
                STATUS.with(|status| *status.borrow_mut() = line);
            }
        }
    });
}

pub fn status() -> String {
    STATUS.with(|status| status.borrow().clone())
}

pub fn provenance() -> String {
    MODULE.with(|slot| {
        slot.borrow()
            .as_ref()
            .map(|loaded| loaded.provenance.clone())
            .unwrap_or_else(status)
    })
}

pub fn loaded_engine_hash() -> Option<[u8; 32]> {
    MODULE.with(|slot| slot.borrow().as_ref().map(|loaded| loaded.hash))
}

pub fn engine_bytes() -> Option<Vec<u8>> {
    MODULE.with(|slot| slot.borrow().as_ref().map(|loaded| loaded.bytes.clone()))
}

pub fn install_engine(bytes: &[u8], provenance: String) -> Result<(), String> {
    let module = compile(bytes)?;
    let hash = *blake3::hash(bytes).as_bytes();
    engine_from_module(&module, hash)?;
    MODULE.with(|slot| {
        *slot.borrow_mut() = Some(LoadedEngine {
            module,
            bytes: bytes.to_vec(),
            hash,
            provenance: provenance.clone(),
        })
    });
    STATUS.with(|status| *status.borrow_mut() = provenance);
    crate::net::node::serve_bytes(bytes.to_vec());
    Ok(())
}

pub fn instantiate_loaded() -> Result<Box<dyn Engine>, String> {
    MODULE.with(|slot| {
        let slot = slot.borrow();
        let loaded = slot.as_ref().ok_or("no engine module loaded")?;
        engine_from_module(&loaded.module, loaded.hash)
    })
}

pub fn engine_from_bytes(bytes: &[u8]) -> Result<Box<dyn Engine>, String> {
    let module = compile(bytes)?;
    engine_from_module(&module, *blake3::hash(bytes).as_bytes())
}

pub fn plugin_with_manifest(
    bytes: &[u8],
) -> Result<(Box<dyn PluginModule>, Option<PluginManifest>), String> {
    let module = compile(bytes)?;
    let mut plugin = AbiPlugin::load(
        WebModule::instantiate(&module, PLUGIN_GAS_BUDGET)?,
        *blake3::hash(bytes).as_bytes(),
    )
    .map_err(|fault| fault.to_string())?;
    let manifest = plugin
        .manifest_bytes()
        .ok()
        .and_then(|bytes| decode_plugin_manifest(&bytes));
    Ok((Box::new(plugin), manifest))
}

fn engine_from_module(
    module: &js_sys::WebAssembly::Module,
    hash: [u8; 32],
) -> Result<Box<dyn Engine>, String> {
    let engine = AbiEngine::load(WebModule::instantiate(module, ENGINE_GAS_BUDGET)?, hash)
        .map_err(|fault| fault.to_string())?;
    Ok(Box::new(engine))
}

fn js_text(value: JsValue) -> String {
    value.as_string().unwrap_or_else(|| format!("{value:?}"))
}

fn compile(bytes: &[u8]) -> Result<js_sys::WebAssembly::Module, String> {
    let buffer = js_sys::Uint8Array::from(bytes);
    js_sys::WebAssembly::Module::new(&buffer.into()).map_err(js_text)
}

async fn fetch_bundled_module() -> Result<(js_sys::WebAssembly::Module, Vec<u8>), String> {
    let window = web_sys::window().ok_or("no window")?;
    let response = JsFuture::from(window.fetch_with_str("./engine.wasm"))
        .await
        .map_err(js_text)?;
    let response: web_sys::Response = response
        .dyn_into()
        .map_err(|_| "fetch returned no response".to_string())?;
    if !response.ok() {
        return Err(format!("fetch answered {}", response.status()));
    }
    let buffer = JsFuture::from(response.array_buffer().map_err(js_text)?)
        .await
        .map_err(js_text)?;
    let bytes = js_sys::Uint8Array::new(&buffer).to_vec();
    let module = JsFuture::from(js_sys::WebAssembly::compile(&buffer))
        .await
        .map_err(js_text)?;
    let module: js_sys::WebAssembly::Module = module
        .dyn_into()
        .map_err(|_| "compile returned no module".to_string())?;
    Ok((module, bytes))
}

pub fn hosting_engine() -> Result<(Box<dyn Engine>, Option<String>), String> {
    match engine_block() {
        Some(reason) => Err(reason),
        None => Ok(session_engine()),
    }
}

pub fn engine_block() -> Option<String> {
    if loaded_engine_hash().is_some() {
        return None;
    }
    let status = status();
    Some(if status.is_empty() {
        "engine.wasm is still loading — try again in a moment".into()
    } else {
        format!("{status} — hosting needs the engine")
    })
}

pub fn session_engine() -> (Box<dyn Engine>, Option<String>) {
    let loaded = MODULE.with(|slot| {
        slot.borrow().as_ref().map(|loaded| {
            (
                loaded.module.clone(),
                loaded.hash,
                loaded.provenance.clone(),
            )
        })
    });
    match loaded {
        Some((module, hash, provenance)) => match engine_from_module(&module, hash) {
            Ok(engine) => (engine, Some(provenance)),
            Err(error) => (
                Box::new(NativeEngine::new()),
                Some(format!("engine refused ({error}) — folding natively")),
            ),
        },
        None => (
            Box::new(NativeEngine::new()),
            Some("engine.wasm not loaded yet — folding natively".into()),
        ),
    }
}

pub struct WebModule {
    exports: SendWrapper<js_sys::Object>,
    memory: SendWrapper<js_sys::WebAssembly::Memory>,
    gas: Option<SendWrapper<js_sys::WebAssembly::Global>>,
    budget: u64,
}

impl WebModule {
    fn instantiate(module: &js_sys::WebAssembly::Module, budget: u64) -> Result<Self, String> {
        let imports = js_sys::Object::new();
        let instance = js_sys::WebAssembly::Instance::new(module, &imports).map_err(js_text)?;
        let exports = instance.exports();
        let memory: js_sys::WebAssembly::Memory = js_sys::Reflect::get(&exports, &"memory".into())
            .map_err(js_text)?
            .dyn_into()
            .map_err(|_| "module exports no memory".to_string())?;
        let gas = js_sys::Reflect::get(&exports, &"gas_left".into())
            .ok()
            .and_then(|value| value.dyn_into::<js_sys::WebAssembly::Global>().ok());
        Ok(Self {
            exports: SendWrapper::new(exports),
            memory: SendWrapper::new(memory),
            gas: gas.map(SendWrapper::new),
            budget,
        })
    }

    fn func(&self, name: &str) -> Result<js_sys::Function, String> {
        js_sys::Reflect::get(&self.exports, &name.into())
            .map_err(js_text)?
            .dyn_into()
            .map_err(|_| format!("module export {name} is not a function"))
    }

    fn gas_left(&self) -> Option<i64> {
        let global = self.gas.as_ref()?;
        let value = global.value();
        let bigint: js_sys::BigInt = value.dyn_into().ok()?;
        i64::try_from(bigint).ok()
    }
}

impl ModuleCall for WebModule {
    fn abi_version(&mut self) -> Result<u32, CallFault> {
        let value = self
            .func("abi_version")
            .and_then(|func| func.call0(&JsValue::UNDEFINED).map_err(js_text))
            .map_err(|error| CallFault::broken("abi_version", error))?;
        value
            .as_f64()
            .map(|number| number as u32)
            .ok_or_else(|| CallFault::Broken("abi_version returned no number".into()))
    }

    fn call(&mut self, name: &str, request: &[u8]) -> Result<Vec<u8>, CallFault> {
        if let Some(gas) = &self.gas {
            gas.set_value(&js_sys::BigInt::from(self.budget as i64).into());
        }
        let len = u32::try_from(request.len())
            .map_err(|_| CallFault::Broken("request exceeds the guest address space".into()))?;
        let ptr = if request.is_empty() {
            0
        } else {
            let ptr = self
                .func("alloc")
                .and_then(|alloc| {
                    alloc
                        .call1(&JsValue::UNDEFINED, &JsValue::from(len))
                        .map_err(js_text)
                })
                .map_err(CallFault::Broken)?;
            let ptr = ptr
                .as_f64()
                .ok_or_else(|| CallFault::Broken("alloc returned no pointer".into()))?
                as u32;
            js_sys::Uint8Array::new(&self.memory.buffer())
                .subarray(ptr, ptr + len)
                .copy_from(request);
            ptr
        };
        let outcome = self.func(name).map_err(CallFault::Broken)?.call2(
            &JsValue::UNDEFINED,
            &JsValue::from(ptr),
            &JsValue::from(len),
        );
        let packed = match outcome {
            Ok(value) => value,
            Err(trap) => {
                if self.gas_left() == Some(-1) {
                    return Err(CallFault::GasExhausted);
                }
                return Err(CallFault::Trapped(js_text(trap)));
            }
        };
        let packed: js_sys::BigInt = packed
            .dyn_into()
            .map_err(|_| CallFault::Broken(format!("{name} returned no reply")))?;
        let packed = i64::try_from(packed)
            .map_err(|_| CallFault::Broken(format!("{name} reply does not fit")))?
            as u64;
        let (reply_ptr, reply_len) = ((packed >> 32) as u32, (packed & 0xffff_ffff) as u32);
        Ok(js_sys::Uint8Array::new(&self.memory.buffer())
            .subarray(reply_ptr, reply_ptr + reply_len)
            .to_vec())
    }
}
