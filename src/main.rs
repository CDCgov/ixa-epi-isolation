use ixa::{context::Context, error::IxaError};

mod gi_sim;

// note this clippy complain will be removed once
// initialize actually does something that may error
#[allow(clippy::unnecessary_wraps)]
fn initialize() -> Result<Context, IxaError> {
    #[allow(unused_mut)]
    let mut context = Context::new();
    gi_sim::init();
    Ok(context)
}

fn main() {
    let mut context = initialize().expect("Error initializing.");
    context.execute();
}
