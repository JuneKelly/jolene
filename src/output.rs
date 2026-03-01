use std::fmt;

#[derive(Debug, Clone, Copy)]
pub struct Output {
    pub verbose: bool,
    pub quiet: bool,
}

impl Output {
    pub fn new(verbose: bool, quiet: bool) -> Output {
        Output { verbose, quiet }
    }

    /// Print a normal message. Suppressed in quiet mode.
    pub fn print(&self, msg: impl fmt::Display) {
        if !self.quiet {
            println!("{msg}");
        }
    }

    /// Print only when verbose. Always suppressed in quiet mode.
    pub fn verbose(&self, msg: impl fmt::Display) {
        if self.verbose && !self.quiet {
            println!("{msg}");
        }
    }

}
