//! Basic types to build the parsers

use self::Needed::*;
use crate::error::{self, ErrorKind};
use core::num::NonZeroUsize;

/// Holds the result of parsing functions
///
/// It depends on the input type `I`, the output type `O`, and the error type `E`
/// (by default `(I, nom::ErrorKind)`)
///
/// The `Ok` side is a pair containing the remainder of the input (the part of the data that
/// was not parsed) and the produced value. The `Err` side contains an instance of `nom::Err`.
///
/// Outside of the parsing code, you can use the [Finish::finish] method to convert
/// it to a more common result type
pub type IResult<I, O, E = error::Error<I>> = Result<(I, O), Err<E>>;

/// Helper trait to convert a parser's result to a more manageable type
pub trait Finish<I,O,E> {
    /// converts the parser's result to a type that is more consumable by error
    /// management libraries. It keeps the same `Ok` branch, and merges `Err::Error`
    /// and `Err::Failure` into the `Err` side.
    ///
    /// *warning*: if the result is `Err(Err::Incomplete(_))`, this method will panic.
    /// - "complete" parsers: It will not be an issue, `Incomplete` is never used
    /// - "streaming" parsers: `Incomplete` will be returned if there's not enough data
    /// for the parser to decide, and you should gather more data before parsing again.
    /// Once the parser returns either `Ok(_)`, `Err(Err::Error(_))` or `Err(Err::Failure(_))`,
    /// you can get out of the parsing loop and call `finish()` on the parser's result
    fn finish(self) -> Result<(I, O), E>;
}

impl<I,O,E> Finish<I,O,E> for IResult<I,O,E> {
    fn finish(self) -> Result<(I, O), E> {
        match self {
            Ok(res) => Ok(res),
            Err(Err::Error(e)) | Err(Err::Failure(e)) => Err(e),
            Err(Err::Incomplete(_)) => {
                panic!("Cannot call `finish()` on `Err(Err::Incomplete(_))`: this result means that the parser does not have enough data to decide, you should gather more data and try to reapply  the parser instead")
            }
        }
    }
}

/// Contains information on needed data if a parser returned `Incomplete`
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Needed {
  /// Needs more data, but we do not know how much
  Unknown,
  /// Contains the required data size in bytes
  Size(NonZeroUsize),
}

impl Needed {
  /// Creates `Needed` instance, returns `Needed::Unknown` if the argument is zero
  pub fn new(s: usize) -> Self {
    match NonZeroUsize::new(s) {
      Some(sz) => Needed::Size(sz),
      None => Needed::Unknown,
    }
  }

  /// Indicates if we know how many bytes we need
  pub fn is_known(&self) -> bool {
    *self != Unknown
  }

  /// Maps a `Needed` to `Needed` by applying a function to a contained `Size` value.
  #[inline]
  pub fn map<F: Fn(NonZeroUsize) -> usize>(self, f: F) -> Needed {
    match self {
      Unknown => Unknown,
      Size(n) => Needed::new(f(n)),
    }
  }
}

/// The `Err` enum indicates the parser was not successful
///
/// It has three cases:
///
/// * `Incomplete` indicates that more data is needed to decide. The `Needed` enum
/// can contain how many additional bytes are necessary. If you are sure your parser
/// is working on full data, you can wrap your parser with the `complete` combinator
/// to transform that case in `Error`
/// * `Error` means some parser did not succeed, but another one might (as an example,
/// when testing different branches of an `alt` combinator)
/// * `Failure` indicates an unrecoverable error. As an example, if you recognize a prefix
/// to decide on the next parser to apply, and that parser fails, you know there's no need
/// to try other parsers, you were already in the right branch, so the data is invalid
///
#[derive(Debug, Clone, PartialEq)]
pub enum Err<E> {
  /// There was not enough data
  Incomplete(Needed),
  /// The parser had an error (recoverable)
  Error(E),
  /// The parser had an unrecoverable error: we got to the right
  /// branch and we know other branches won't work, so backtrack
  /// as fast as possible
  Failure(E),
}

impl<E> Err<E> {
  /// Tests if the result is Incomplete
  pub fn is_incomplete(&self) -> bool {
    if let Err::Incomplete(_) = self {
      true
    } else {
      false
    }
  }

  /// Applies the given function to the inner error
  pub fn map<E2, F>(self, f: F) -> Err<E2>
  where
    F: FnOnce(E) -> E2,
  {
    match self {
      Err::Incomplete(n) => Err::Incomplete(n),
      Err::Failure(t) => Err::Failure(f(t)),
      Err::Error(t) => Err::Error(f(t)),
    }
  }

  /// Automatically converts between errors if the underlying type supports it
  pub fn convert<F>(e: Err<F>) -> Self
  where
    E: From<F>,
  {
    e.map(Into::into)
  }
}

impl<T> Err<(T, ErrorKind)> {
  /// Maps `Err<(T, ErrorKind)>` to `Err<(U, ErrorKind)>` with the given `F: T -> U`
  pub fn map_input<U, F>(self, f: F) -> Err<(U, ErrorKind)>
  where
    F: FnOnce(T) -> U,
  {
    match self {
      Err::Incomplete(n) => Err::Incomplete(n),
      Err::Failure((input, k)) => Err::Failure((f(input), k)),
      Err::Error((input, k)) => Err::Error((f(input), k)),
    }
  }
}

#[cfg(feature = "alloc")]
use crate::lib::std::{vec::Vec, string::String, borrow::ToOwned};
#[cfg(feature = "alloc")]
impl Err<(&[u8], ErrorKind)> {
  /// Obtaining ownership
  pub fn to_owned(self) -> Err<(Vec<u8>, ErrorKind)> {
    self.map_input(ToOwned::to_owned)
  }
}

#[cfg(feature = "alloc")]
impl Err<(&str, ErrorKind)> {
  /// Automatically converts between errors if the underlying type supports it
  pub fn to_owned(self) -> Err<(String, ErrorKind)> {
    self.map_input(ToOwned::to_owned)
  }
}

impl<E: Eq> Eq for Err<E> {}

#[cfg(feature = "std")]
use std::fmt;

#[cfg(feature = "std")]
impl<E> fmt::Display for Err<E>
where
  E: fmt::Debug,
{
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    match self {
      Err::Incomplete(Needed::Size(u)) => write!(f, "Parsing requires {} bytes/chars", u),
      Err::Incomplete(Needed::Unknown) => write!(f, "Parsing requires more data"),
      Err::Failure(c) => write!(f, "Parsing Failure: {:?}", c),
      Err::Error(c) => write!(f, "Parsing Error: {:?}", c),
    }
  }
}

#[cfg(feature = "std")]
use std::error::Error;

#[cfg(feature = "std")]
impl<E> Error for Err<E>
where
  E: fmt::Debug,
{
  fn source(&self) -> Option<&(dyn Error + 'static)> {
    None // no underlying error
  }
}

/// All nom parsers implement this trait
pub trait Parser<I, O, E> {
  /// A parser takes in input type, and returns a `Result` containing
  /// either the remaining input and the output value, or an error
  fn parse(&mut self, input: I) -> IResult<I, O, E>;

  /// Maps a function over the result of a parser
  fn map<G, O2>(self, g: G) -> Map<Self, G, O>
  where
    G: Fn(O) -> O2,
    Self: core::marker::Sized,
  {
    Map {
      f: self,
      g,
      phantom: core::marker::PhantomData,
    }
  }

  /// Creates a second parser from the output of the first one, then apply over the rest of the input
  fn flat_map<G, H, O2>(self, g: G) -> FlatMap<Self, G, O>
  where
    G: Fn(O) -> H,
    H: Parser<I, O2, E>,
    Self: core::marker::Sized,
  {
    FlatMap {
      f: self,
      g,
      phantom: core::marker::PhantomData,
    }
  }

  /// Applies a second parser over the output of the first one
  fn and_then<G, O2>(self, g: G) -> AndThen<Self, G, O>
  where
    G: Parser<O, O2, E>,
    Self: core::marker::Sized,
  {
    AndThen {
      f: self,
      g,
      phantom: core::marker::PhantomData,
    }
  }

  /// Applies a second parser after the first one, return their results as a tuple
  fn and<G, O2>(self, g: G) -> And<Self, G>
  where
    G: Parser<I, O2, E>,
    Self: core::marker::Sized,
  {
    And { f: self, g }
  }

  /// Applies a second parser over the input if the first one failed
  fn or<G>(self, g: G) -> Or<Self, G>
  where
    G: Parser<I, O, E>,
    Self: core::marker::Sized,
  {
    Or { f: self, g }
  }
}

impl<'a, I, O, E, F> Parser<I, O, E> for F
where
  F: FnMut(I) -> IResult<I, O, E> + 'a,
{
  fn parse(&mut self, i: I) -> IResult<I, O, E> {
    self(i)
  }
}

/// Implementation of `Parser:::map`
pub struct Map<F, G, O1> {
  f: F,
  g: G,
  phantom: core::marker::PhantomData<O1>,
}

impl<'a, I, O1, O2, E, F: Parser<I, O1, E>, G: Fn(O1) -> O2> Parser<I, O2, E> for Map<F, G, O1> {
  fn parse(&mut self, i: I) -> IResult<I, O2, E> {
    match self.f.parse(i) {
      Err(e) => Err(e),
      Ok((i, o)) => Ok((i, (self.g)(o))),
    }
  }
}

/// Implementation of `Parser::flat_map`
pub struct FlatMap<F, G, O1> {
  f: F,
  g: G,
  phantom: core::marker::PhantomData<O1>,
}

impl<'a, I, O1, O2, E, F: Parser<I, O1, E>, G: Fn(O1) -> H, H: Parser<I, O2, E>> Parser<I, O2, E>
  for FlatMap<F, G, O1>
{
  fn parse(&mut self, i: I) -> IResult<I, O2, E> {
    let (i, o1) = self.f.parse(i)?;
    (self.g)(o1).parse(i)
  }
}

/// Implementation of `Parser::and_then`
pub struct AndThen<F, G, O1> {
  f: F,
  g: G,
  phantom: core::marker::PhantomData<O1>,
}

impl<'a, I, O1, O2, E, F: Parser<I, O1, E>, G: Parser<O1, O2, E>> Parser<I, O2, E>
  for AndThen<F, G, O1>
{
  fn parse(&mut self, i: I) -> IResult<I, O2, E> {
    let (i, o1) = self.f.parse(i)?;
    let (_, o2) = self.g.parse(o1)?;
    Ok((i, o2))
  }
}

/// Implementation of `Parser::and`
pub struct And<F, G> {
  f: F,
  g: G,
}

impl<'a, I, O1, O2, E, F: Parser<I, O1, E>, G: Parser<I, O2, E>> Parser<I, (O1, O2), E>
  for And<F, G>
{
  fn parse(&mut self, i: I) -> IResult<I, (O1, O2), E> {
    let (i, o1) = self.f.parse(i)?;
    let (i, o2) = self.g.parse(i)?;
    Ok((i, (o1, o2)))
  }
}

/// Implementation of `Parser::or`
pub struct Or<F, G> {
  f: F,
  g: G,
}

impl<'a, I: Clone, O, E: crate::error::ParseError<I>, F: Parser<I, O, E>, G: Parser<I, O, E>>
  Parser<I, O, E> for Or<F, G>
{
  fn parse(&mut self, i: I) -> IResult<I, O, E> {
    match self.f.parse(i.clone()) {
      Err(Err::Error(e1)) => match self.g.parse(i) {
        Err(Err::Error(e2)) => Err(Err::Error(e1.or(e2))),
        res => res,
      },
      res => res,
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::error::ErrorKind;

  #[doc(hidden)]
  #[macro_export]
  macro_rules! assert_size (
    ($t:ty, $sz:expr) => (
      assert_eq!(crate::lib::std::mem::size_of::<$t>(), $sz);
    );
  );

  #[test]
  #[cfg(target_pointer_width = "64")]
  fn size_test() {
    assert_size!(IResult<&[u8], &[u8], (&[u8], u32)>, 40);
    assert_size!(IResult<&str, &str, u32>, 40);
    assert_size!(Needed, 8);
    assert_size!(Err<u32>, 16);
    assert_size!(ErrorKind, 1);
  }

  #[test]
  fn err_map_test() {
    let e = Err::Error(1);
    assert_eq!(e.map(|v| v + 1), Err::Error(2));
  }
}
