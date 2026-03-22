/// A trait for types that can be sanitized.
///
/// Types implementing this trait can be processed to produce a sanitized output.
/// The `sanitize` method is used to perform this operation.
pub trait Sanitizable {
    /// Sanitizes the current instance and returns the sanitized output.
    fn sanitize(self) -> Self;
}
