use crate::error::Error;
use crate::primitives::FixedSize;
use alloc::boxed::Box;
use core::convert::TryFrom;
use serde::{de::Visitor, ser, Deserialize, Deserializer, Serialize};
use crate::primitives::Vec;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Inner<'a> {
    Ref(&'a [u8]),
    Owned(Box<[u8; 32]>),
}

enum Inner_as_ref<'a> {
    Ref(&'a [u8]),
    Owned(Box<[u8; 32]>),
}

impl<'a> Inner_as_ref<'a> {
    #[inline]
    pub fn inner_as_ref(&'a self) -> &'a [u8] {
        match self {
            Self::Ref(v) => v,
            Self::Owned(v) => &v[..],
        }
    }
    
    
    
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct U256<'u>(Inner<'u>);

impl<'u> TryFrom<&'u [u8]> for U256<'u> {
    type Error = Error;

    #[inline]
    fn try_from(v: &'u [u8]) -> core::result::Result<Self, Error> {
        if v.len() == 32 {
            Ok(Self(Inner::Ref(v)))
        } else {
            Err(Error::InvalidU256(v.len()))
        }
    }
}

impl<'u> TryFrom<&'u mut [u8]> for U256<'u> {
    type Error = Error;

    #[inline]
    fn try_from(v: &'u mut [u8]) -> core::result::Result<Self, Error> {
        if v.len() == 32 {
            Ok(Self(Inner::Ref(v)))
        } else {
            Err(Error::InvalidU256(v.len()))
        }
    }
}

impl<'u> From<[u8; 32]> for U256<'u> {
    fn from(v: [u8; 32]) -> Self {
        U256(Inner::Owned(Box::new(v)))
    }
}

impl<'u> From<&'u U256<'u>> for &'u [u8] {
    #[inline]
    fn from(v: &'u U256<'u>) -> Self {
        match &v.0 {
            Inner::Ref(v) => v,
            Inner::Owned(v) => &v[..],
        }
    }
}

impl<'u> Serialize for U256<'u> {
    #[inline]
    fn serialize<S>(&self, serializer: S) -> core::result::Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        serializer.serialize_bytes(self.into())
    }
}

struct U256Visitor;

impl<'a> Visitor<'a> for U256Visitor {
    type Value = U256<'a>;

    fn expecting(&self, formatter: &mut core::fmt::Formatter) -> core::fmt::Result {
        formatter.write_str("a 32 bytes unsigned le int")
    }

    #[inline]
    fn visit_borrowed_bytes<E>(self, value: &'a [u8]) -> Result<Self::Value, E> {
        Ok(U256(Inner::Ref(value)))
    }
}

impl<'de: 'a, 'a> Deserialize<'de> for U256<'a> {
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_newtype_struct("U256", U256Visitor)
    }
}

impl<'a> FixedSize for U256<'a> {
    const FIXED_SIZE: usize = 32;
}

// use core::convert::TryInto;

// impl TryFrom<usize> for U256 {
//     type Error = crate::Error;

//     fn try_from(v: usize) -> Result<Self, Self::Error> {
//         let v: u32 = v
//             .try_into()
//             .map_err(|_| crate::Error::U24TooBig(u32::MAX))?;
//         match v {
//             0..=Self::MAX => Ok(Self(v)),
//             _ => Err(crate::Error::U24TooBig(v)),
//         }
//     }
// }


