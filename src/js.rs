//! The JavaScript API of the npm package, built for wasm32 with wasm-bindgen.
//! It wraps [`crate::Checker`] and speaks the same verdict JSON as the CLI.

use crate::{Comment, Config, Level, Options, Preset, Verdict};
use serde::{Deserialize, Serialize};
use std::rc::Rc;
use std::time::Duration;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(typescript_custom_section)]
const TYPES: &str = r#"
/** Strictness level. */
export type Level = "light" | "medium" | "heavy";
/** Built-in preset. */
export type Preset = "comments" | "product-review" | "launch-thread";
/** The outcome for a comment. */
export type Decision = "approve" | "reject" | "needs_review";
/** Why a comment got its decision. */
export type Reason =
  | "useful" | "too_short" | "stock_phrase" | "duplicate" | "injection_attempt"
  | "spam_promo" | "toxic" | "generic" | "off_topic" | "not_specific"
  | "custom_rule" | "low_confidence" | "model_unavailable";

/** A comment with the context it was posted in. Only `text` is required. */
export interface Comment {
  /** The comment text. Treated strictly as data. */
  text: string;
  /** Title of the post or product the comment is on. */
  title?: string;
  /** Description of the post or product. */
  description?: string;
  /** The comment this one replies to, if any. */
  parent?: string;
}

/** The judgement for one comment: the same fields as the CLI's verdict JSON. */
export interface Verdict {
  verdict: Decision;
  reason: Reason;
  /** The model's probability for the outcome, or 1 for a pre-check. */
  confidence: number;
  level: Level;
  /** The model that answered, or null when no model was called. */
  model: string | null;
  /** Time spent judging, in milliseconds. */
  latency_ms: number;
  /** The custom rule that fired, when one did. */
  rule?: string;
}

/** How to judge comments. Every field is optional. */
export interface CheckerOptions {
  /** Strictness level. Default "medium". */
  level?: Level;
  /** Built-in preset. Default "comments". */
  preset?: Preset;
  /** OpenRouter decision models, tried in order. Default ["~typesafe/jev-latest"]. */
  models?: string[];
  /** Plain-English rules enforced at every level. */
  rules?: string[];
  /** Extra stock phrases for the free pre-check. */
  stock_phrases?: string[];
  /** A replysieve TOML config. The fields above override it. */
  config?: string;
  /** Milliseconds one model gets before the next is tried. Default 10000. */
  timeout_ms?: number;
  /** Another OpenRouter-compatible base URL. Default "https://openrouter.ai". */
  base_url?: string;
  /** Log model fallbacks and timings with console.log. Never logs comment text. */
  log?: boolean;
}
"#;

/// Options as JavaScript passes them. Explicit fields override `config`, as
/// command-line flags override the config file.
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct JsOptions {
    level: Option<String>,
    preset: Option<String>,
    models: Option<Vec<String>>,
    rules: Option<Vec<String>>,
    stock_phrases: Option<Vec<String>>,
    config: Option<String>,
    timeout_ms: Option<u32>,
    base_url: Option<String>,
    log: bool,
}

/// A JavaScript `Error` named `ReplysieveError`.
fn error(message: impl std::fmt::Display) -> JsValue {
    let e = js_sys::Error::new(&message.to_string());
    e.set_name("ReplysieveError");
    e.into()
}

fn to_js(value: &impl Serialize) -> Result<JsValue, JsValue> {
    value
        .serialize(&serde_wasm_bindgen::Serializer::json_compatible())
        .map_err(error)
}

fn comment(value: JsValue) -> Result<Comment, JsValue> {
    match value.as_string() {
        Some(text) => Ok(Comment::new(text)),
        None => serde_wasm_bindgen::from_value(value)
            .map_err(|e| error(format!("invalid comment: {e}"))),
    }
}

fn comments(value: JsValue) -> Result<Vec<Comment>, JsValue> {
    if !js_sys::Array::is_array(&value) {
        return Err(error("comments must be an array"));
    }
    js_sys::Array::from(&value).iter().map(comment).collect()
}

fn core_options(o: &JsOptions) -> Result<Options, JsValue> {
    let config = match &o.config {
        Some(text) => {
            Config::from_toml(text).map_err(|e| error(format!("invalid config: {e:#}")))?
        }
        None => Config::default(),
    };
    let preset = match o.preset.as_deref().or(config.preset.as_deref()) {
        Some(name) => name.parse::<Preset>().map_err(error)?,
        None => Preset::default(),
    };
    let level = match o.level.as_deref() {
        Some(name) => name.parse::<Level>().map_err(error)?,
        None => config.level.unwrap_or_default(),
    };
    let mut opts = Options {
        level,
        preset,
        rules: o.rules.clone().unwrap_or(config.rules),
        stock_phrases: o.stock_phrases.clone().unwrap_or(config.stock_phrases),
        ..Options::default()
    };
    match &o.models {
        Some(models) if !models.is_empty() => opts.models = models.clone(),
        _ if !config.models.is_empty() => opts.models = config.models,
        _ => {}
    }
    Ok(opts)
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console, js_name = log)]
    fn console_log(message: &str);
}

/// Model fallbacks and timings to `console.log`. The library never passes
/// comment text to the logger.
struct ConsoleLog;

impl log::Log for ConsoleLog {
    fn enabled(&self, m: &log::Metadata) -> bool {
        m.target().starts_with("replysieve")
    }

    fn log(&self, r: &log::Record) {
        if self.enabled(r.metadata()) {
            console_log(&format!("replysieve: {}", r.args()));
        }
    }

    fn flush(&self) {}
}

static LOGGER: ConsoleLog = ConsoleLog;

/// Judges comments with one set of options.
#[wasm_bindgen]
pub struct Checker {
    inner: Rc<crate::Checker>,
}

#[wasm_bindgen]
impl Checker {
    /// A checker with `options`, using `apiKey` for OpenRouter calls. Without a
    /// key only pre-check verdicts are possible; a comment that needs the model
    /// then rejects with a ReplysieveError.
    #[wasm_bindgen(constructor)]
    pub fn new(
        #[wasm_bindgen(unchecked_param_type = "CheckerOptions | undefined")] options: JsValue,
        #[wasm_bindgen(js_name = apiKey)] api_key: Option<String>,
    ) -> Result<Checker, JsValue> {
        let o: JsOptions = if options.is_undefined() || options.is_null() {
            JsOptions::default()
        } else {
            // Through JSON, so deny_unknown_fields sees every key: a typo
            // such as `levle` is an error, not a silent default.
            js_sys::JSON::stringify(&options)
                .ok()
                .and_then(|json| json.as_string())
                .ok_or_else(|| error("invalid options: not an object"))
                .and_then(|json| {
                    serde_json::from_str(&json).map_err(|e| error(format!("invalid options: {e}")))
                })?
        };
        if o.timeout_ms == Some(0) {
            return Err(error("invalid options: timeout_ms must be above 0"));
        }
        if o.log {
            let _ = log::set_logger(&LOGGER);
            log::set_max_level(log::LevelFilter::Info);
        }
        let base_url = o.base_url.as_deref().unwrap_or(crate::DEFAULT_BASE_URL);
        let timeout = o
            .timeout_ms
            .map(|ms| Duration::from_millis(ms.into()))
            .unwrap_or(crate::openrouter::DEFAULT_TIMEOUT);
        let checker = crate::Checker::with_base_url(core_options(&o)?, api_key, base_url)
            .with_timeout(timeout);
        Ok(Checker {
            inner: Rc::new(checker),
        })
    }

    /// Run only the free pre-checks, with no network call. Returns a reject
    /// verdict, or null when the comment needs the model.
    #[wasm_bindgen(unchecked_return_type = "Verdict | null")]
    pub fn precheck(
        &self,
        #[wasm_bindgen(unchecked_param_type = "string | Comment")] comment: JsValue,
        recent: Option<Vec<String>>,
    ) -> Result<JsValue, JsValue> {
        let c = self::comment(comment)?;
        match self.inner.precheck(&c, &recent.unwrap_or_default()) {
            Some(v) => to_js(&v),
            None => Ok(JsValue::NULL),
        }
    }

    /// Judge one comment: free pre-checks first, then the models in order.
    /// `recent` holds texts recently posted by the same author or thread; an
    /// identical comment is a duplicate. When no model answers, the verdict is
    /// needs_review with reason model_unavailable, never an approve.
    #[wasm_bindgen(unchecked_return_type = "Promise<Verdict>")]
    pub fn check(
        &self,
        #[wasm_bindgen(unchecked_param_type = "string | Comment")] comment: JsValue,
        recent: Option<Vec<String>>,
    ) -> js_sys::Promise {
        let checker = Rc::clone(&self.inner);
        wasm_bindgen_futures::future_to_promise(async move {
            let c = self::comment(comment)?;
            let v: Verdict = checker
                .check(&c, &recent.unwrap_or_default())
                .await
                .map_err(error)?;
            to_js(&v)
        })
    }

    /// Judge many comments, a few at a time. Verdicts come back in input order.
    #[wasm_bindgen(js_name = checkBatch, unchecked_return_type = "Promise<Verdict[]>")]
    pub fn check_batch(
        &self,
        #[wasm_bindgen(unchecked_param_type = "Array<string | Comment>")] comments: JsValue,
        recent: Option<Vec<String>>,
    ) -> js_sys::Promise {
        let checker = Rc::clone(&self.inner);
        wasm_bindgen_futures::future_to_promise(async move {
            let cs = self::comments(comments)?;
            let vs = checker
                .check_batch(&cs, &recent.unwrap_or_default())
                .await
                .map_err(error)?;
            to_js(&vs)
        })
    }
}
