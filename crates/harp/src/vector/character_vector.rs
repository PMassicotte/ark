//
// character_vector.rs
//
// Copyright (C) 2022 Posit Software, PBC. All rights reserved.
//
//

use std::ffi::CStr;

use libR_sys::*;

use crate::object::RObject;
use crate::traits::AsSlice;
use crate::vector::Vector;

#[harp_macros::vector]
pub struct CharacterVector {
    object: RObject,
}

impl Vector for CharacterVector {
    type Item = str;
    type Type = &'static str;
    const SEXPTYPE: u32 = STRSXP;

    fn data(&self) -> SEXP {
        self.object.sexp
    }

    unsafe fn new_unchecked(object: impl Into<SEXP>) -> Self {
        let object = object.into();
        Self {
            object: RObject::new(object),
        }
    }

    unsafe fn create<T>(data: T) -> Self
    where
        T: IntoIterator,
        <T as IntoIterator>::IntoIter: ExactSizeIterator,
        <T as IntoIterator>::Item: AsRef<Self::Item>,
    {
        // convert into iterator
        let mut data = data.into_iter();

        // build our character vector
        let n = data.len();
        let vector = CharacterVector::with_length(n);
        for i in 0..data.len() {
            let value = data.next().unwrap_unchecked();
            let value = value.as_ref();
            let charsexp = Rf_mkCharLenCE(
                value.as_ptr() as *const i8,
                value.len() as i32,
                cetype_t_CE_UTF8,
            );
            SET_STRING_ELT(vector.data(), i as R_xlen_t, charsexp);
        }
        vector
    }

    unsafe fn get_unchecked(&self, index: isize) -> Self::Type {
        let data = self.data();
        let charsxp = STRING_ELT(data, index as R_xlen_t);
        let cstr = Rf_translateCharUTF8(charsxp);
        let bytes = CStr::from_ptr(cstr).to_bytes();
        std::str::from_utf8_unchecked(bytes)
    }

}

#[cfg(test)]
mod test {
    use crate::r_test;
    use crate::utils::r_typeof;
    use crate::vector::*;


    #[test]
    fn test_character_vector() {
        r_test! {

            let vector = CharacterVector::create(&["hello", "world"]);
            assert!(vector == ["hello", "world"]);
            assert!(vector == &["hello", "world"]);

            let mut it = vector.iter();

            let value = it.next();
            assert!(value.is_some());
            assert!(value.unwrap() == "hello");

            let value = it.next();
            assert!(value.is_some());
            assert!(value.unwrap() == "world");

            let value = it.next();
            assert!(value.is_none());

            let vector = CharacterVector::create([
                "hello".to_string(),
                "world".to_string()
            ]);

            assert!(vector.get_unchecked(0) == "hello");
            assert!(vector.get_unchecked(1) == "world");

        }
    }

    #[test]
    fn test_create() {
        r_test! {

            let expected = ["Apple", "Orange", "한"];
            let vector = CharacterVector::create(&expected);
            assert_eq!(vector.get(0).unwrap(), "Apple");
            assert_eq!(vector.get(1).unwrap(), "Orange");
            assert_eq!(vector.get(2).unwrap(), "한");

            let alphabet = ["a", "b", "c"];

            // &[&str]
            let s = CharacterVector::create(&alphabet);
            assert_eq!(r_typeof(*s), STRSXP);
            assert_eq!(s, alphabet);

            // &[&str; N]
            let s = CharacterVector::create(&alphabet[..]);
            assert_eq!(r_typeof(*s), STRSXP);
            assert_eq!(s, alphabet);

            // Vec<String>
            let s = CharacterVector::create(alphabet.to_vec());
            assert_eq!(r_typeof(*s), STRSXP);
            assert_eq!(s, alphabet);

        }
    }


}
