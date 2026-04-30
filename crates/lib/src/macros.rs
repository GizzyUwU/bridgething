/// Generate `From<Leaf> for Outer` impls that lift through one or more
/// intermediate variant constructors. `derive_more::From` only produces
/// single-step lifts; this composes them so a leaf can `.into()` straight to
/// the root in one expression.
///
/// Each entry is `Leaf => Outer: |v| <ctor expression>` where the closure body
/// names the full constructor chain that wraps `v` into `Outer`.
///
/// ```ignore
/// transitive_from! {
///   StockVersionSend => PossibleSendMsg: |v| PossibleSendMsg::Stock(StockSendMsg::Version(v)),
///   StockSetupSend   => PossibleSendMsg: |v| PossibleSendMsg::Stock(StockSendMsg::Setup(v)),
/// }
/// ```
#[macro_export]
macro_rules! transitive_from {
  ($($leaf:ty => $outer:ty : $ctor:expr),+ $(,)?) => {
    $(
      impl ::core::convert::From<$leaf> for $outer {
        fn from(value: $leaf) -> Self {
          ($ctor)(value)
        }
      }
    )+
  };
}
