pub trait TVerifiableByVerifyMacro {
    fn is_verify_true(&self) -> bool;
}

impl<T> TVerifiableByVerifyMacro for Option<T> {
    fn is_verify_true(&self) -> bool {
        self.is_some()
    }
}

impl<TOk, TErr> TVerifiableByVerifyMacro for Result<TOk, TErr> {
    fn is_verify_true(&self) -> bool {
        self.is_ok()
    }
}

macro_rules! verify {($e: expr) => {
    {
        let e = $e;
        assert!(e.is_verify_true(), "verify!({}): {:?}", stringify!($e), e);;
        e
    }
}}

macro_rules! verify_eq {($e: expr, $e_chk: expr) => {
    {
        let e = $e;
        assert_eq!(e, $e_chk);;
        e
    }
}}

#[cfg(debug_assertions)]
macro_rules! debug_verify_eq {($e: expr, $e_chk: expr) => {
    verify_eq!($e, $e_chk)
}}
#[cfg(not(debug_assertions))]
macro_rules! debug_verify_eq {($e: expr, $e_chk: expr) => {
    $e
}}

#[test]
fn test_verify() {
    verify!(Some(4));
}
