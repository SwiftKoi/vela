//! The pass interface, and the pipeline that runs it.

use crate::ir::Module;

use crate::opt::OptLevel;

/// Runs the pipeline over a module.
pub fn optimize(module: &mut Module, level: OptLevel) {
    if level == OptLevel::None {
        return;
    }
    for pass in crate::opt::passes() {
        if level.includes(pass.level()) {
            pass.run(module);
        }
    }
}

/// One transformation.
pub trait Pass {
    /// The pass's name, which is what a `-O` report prints.
    fn name(&self) -> &'static str;

    /// The lowest level at which the pass runs.
    fn level(&self) -> OptLevel;

    /// Transforms a module in place.
    fn run(&self, module: &mut Module);
}
