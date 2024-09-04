use std::path::PathBuf;

use handlebars::{
    Context, Handlebars, Helper, HelperDef, RenderContext, RenderError, RenderErrorReason,
    ScopedJson,
};
use serde_json::Value;

pub struct IncludeStatic {
    pub base_dir: PathBuf,
}

impl HelperDef for IncludeStatic {
    fn call_inner<'reg: 'rc, 'rc>(
        &self,
        helper: &Helper<'rc>,
        _: &'reg Handlebars<'reg>,
        _: &'rc Context,
        _: &mut RenderContext<'reg, 'rc>,
    ) -> Result<ScopedJson<'rc>, RenderError> {
        if let Some(param) = helper.param(0).and_then(|v| v.value().as_str()) {
            return Ok(ScopedJson::Derived(Value::String(
                std::fs::read_to_string(self.base_dir.join(param)).map_err(|error| {
                    RenderErrorReason::Other(format!("cannot read static asset {param}: {error}"))
                })?,
            )));
        }

        Err(RenderErrorReason::Other("asset path must be provided".into()).into())
    }
}
