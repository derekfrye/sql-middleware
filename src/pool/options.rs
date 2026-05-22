/// Shared options for `bb8`-backed middleware pools.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MiddlewarePoolOptions {
    /// Whether `bb8` should call the backend's validation hook before checkout.
    pub test_on_check_out: bool,
}

impl Default for MiddlewarePoolOptions {
    fn default() -> Self {
        Self {
            test_on_check_out: true,
        }
    }
}

impl MiddlewarePoolOptions {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_test_on_check_out(mut self, test_on_check_out: bool) -> Self {
        self.test_on_check_out = test_on_check_out;
        self
    }

    pub(crate) fn apply_to<M>(self, builder: bb8::Builder<M>) -> bb8::Builder<M>
    where
        M: bb8::ManageConnection,
    {
        builder.test_on_check_out(self.test_on_check_out)
    }
}
