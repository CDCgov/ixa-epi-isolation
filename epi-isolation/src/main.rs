use ixa::{context::Context, error::IxaError};

// note this clippy complain will be removed once
// initialize actually does something that may error
#[allow(clippy::unnecessary_wraps)]
fn initialize() -> Result<Context, IxaError> {
    let mut context = Context::new();
    context.add_plan(0.0, |_context| {});
    Ok(context)
}

fn main() {
    let mut context = initialize().expect("Error initializing.");
    context.execute();
}
