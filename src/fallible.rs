//! [`Fallible`], the output of an operation that may fail.

use crate::NoValue;

/// The output of an operation that may fail.
///
/// A retry converts the output of each try with [`into_result`](Fallible::into_result), and
/// returns the first success. After a failure, it tries again, unless `when` rejects the failure,
/// no delays are left, or the deadline leaves no time. In those cases it returns a
/// [`RetryError`](crate::RetryError) that holds the failure.
///
/// It is implemented for:
///
/// - `Result`, which fails with its error, and
/// - `Option`, which fails on `None`, with the failure [`NoValue`].
///
/// An `Option<Result<T, E>>`, such as the next item of a stream of results, succeeds with
/// `Some(Err(error))`. Retry its errors by turning it into a `Result<Option<T>, E>` with
/// [`transpose`](Option::transpose).
///
/// Implement it for an output of your own, for example a response whose server errors are failures
/// worth another try:
///
/// ```
/// # #[cfg(feature = "std")] {
/// use try_again::delay::Fixed;
/// use try_again::{retry, DurationExt, Fallible};
///
/// #[derive(Debug, PartialEq)]
/// struct Response {
///     status: u16,
/// }
///
/// impl Fallible for Response {
///     type Success = Response;
///     type Failure = u16;
///
///     fn into_result(self) -> Result<Response, u16> {
///         if self.status >= 500 { Err(self.status) } else { Ok(self) }
///     }
/// }
///
/// let mut statuses = [503, 502, 404].into_iter();
/// let out = retry(|| Response { status: statuses.next().unwrap_or(200) })
///     .delays(Fixed::of(1.milliseconds()).take(5))
///     .run();
///
/// // A client error is the server's answer, which another try would not change.
/// assert_eq!(out, Ok(Response { status: 404 }));
/// # }
/// ```
#[diagnostic::on_unimplemented(
    message = "`{Self}` does not say whether the operation failed",
    label = "the output of the operation",
    note = "a retried operation must return a `try_again::Fallible`, such as a `Result` or an \
            `Option`",
    note = "to retry an async operation, which returns a future, use `retry_async`"
)]
pub trait Fallible {
    /// The value of a successful output, for example the `T` of a `Result<T, E>`.
    type Success;

    /// The value of a failed output, for example the `E` of a `Result<T, E>`.
    type Failure;

    /// Converts the output into a `Result`, the success or the failure.
    ///
    /// # Errors
    ///
    /// Returns the failure if the operation failed.
    fn into_result(self) -> Result<Self::Success, Self::Failure>;
}

impl<T, E> Fallible for Result<T, E> {
    type Success = T;
    type Failure = E;

    fn into_result(self) -> Self {
        self
    }
}

impl<T> Fallible for Option<T> {
    type Success = T;
    type Failure = NoValue;

    fn into_result(self) -> Result<T, NoValue> {
        self.ok_or(NoValue)
    }
}

#[cfg(test)]
mod tests {
    use super::{Fallible, NoValue};
    use assertr::prelude::*;

    #[test]
    fn a_result_keeps_its_value_and_its_error() {
        let ok: Result<u8, i32> = Ok(1);
        let err: Result<u8, i32> = Err(42);

        assert_that!(ok.into_result()).is_equal_to(Ok(1));
        assert_that!(err.into_result()).is_equal_to(Err(42));
    }

    #[test]
    fn an_option_fails_with_no_value_when_none() {
        let some = Some(2);
        let none: Option<u8> = None;

        assert_that!(some.into_result()).is_equal_to(Ok(2));
        assert_that!(none.into_result()).is_equal_to(Err(NoValue));
    }
}
