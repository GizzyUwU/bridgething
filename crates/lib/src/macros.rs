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
