use std::{collections::HashMap, ops::Deref};

use serde::{
    de::{Error, Visitor},
    Deserialize,
};

use crate::emulator::{GPRegister, MemoryData};

#[derive(Debug, Clone)]
pub enum TestCheck {
    Register(GPRegister, u32),
    Memory(u32, MemoryData),
}

fn try_parse_hex_or_dec(s: &str) -> Option<u32> {
    s.strip_prefix("0x")
        .map_or_else(|| s.parse().ok(), |end| u32::from_str_radix(end, 16).ok())
}

struct TestDataVisitor;
impl<'de> Visitor<'de> for TestDataVisitor {
    type Value = Vec<TestCheck>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(formatter, "Either a register as rXX or ??")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        let mut res = Vec::with_capacity(map.size_hint().unwrap_or(0));
        while let Some(key) = map.next_key::<&str>()? {
            if key.starts_with(['r', 'R']) {
                let rn = key[1..]
                    .parse::<u8>()
                    .ok()
                    .and_then(GPRegister::new)
                    .ok_or_else(|| {
                        A::Error::invalid_type(
                            serde::de::Unexpected::Other("unknown register"),
                            &"a register (r0-r31)",
                        )
                    })?;
                let value: u32 = map.next_value()?;
                res.push(TestCheck::Register(rn, value))
            } else if key.starts_with(['m', 'M']) {
                let addr = try_parse_hex_or_dec(&key[2..(key.len() - 1)]).ok_or_else(|| {
                    A::Error::invalid_type(
                        serde::de::Unexpected::Other("unknown memory address"),
                        &"a valid address",
                    )
                })?;
                let value = map.next_value::<MemoryData>()?;
                res.push(TestCheck::Memory(addr, value));
            } else {
                return Err(A::Error::invalid_type(
                    serde::de::Unexpected::Other("unknown type to test"),
                    &"a register",
                ));
            }
        }
        Ok(res)
    }
}

#[derive(Debug, Clone)]
pub struct TestChecks(pub Vec<TestCheck>);

impl<'de> Deserialize<'de> for TestChecks {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(Self(deserializer.deserialize_map(TestDataVisitor)?))
    }
}

impl Deref for TestChecks {
    type Target = [TestCheck];

    fn deref(&self) -> &Self::Target {
        self.0.as_slice()
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum TestData {
    NoSetup(TestChecks),
    WithSetup {
        entrypoint: Option<String>,
        name: Option<String>,
        setup: TestChecks,
        checks: TestChecks,
    },
}

impl TestData {
    pub fn get_name(&self) -> Option<&str> {
        match self {
            Self::NoSetup(_) => None,
            Self::WithSetup {
                entrypoint: _,
                name,
                setup: _,
                checks: _,
            } => name.as_deref(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum Test {
    Single(TestData),
    Multiple(Vec<TestData>),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(transparent)]
pub struct Tests(HashMap<String, Test>);

impl Tests {
    pub fn get_tests(
        &self,
    ) -> impl Iterator<Item = (String, impl Iterator<Item = (String, &TestData)>)> {
        self.0
            .iter()
            .map::<(String, Box<dyn Iterator<Item = _>>), _>(|(name, val)| {
                (
                    name.to_string(),
                    match val {
                        Test::Single(test) => {
                            Box::new(std::iter::once((format!("test_{name}"), test)))
                        }
                        Test::Multiple(many) => {
                            Box::new(many.iter().enumerate().map(move |(index, test)| {
                                (
                                    test.get_name().map_or_else(
                                        || format!("test_{name}{index}"),
                                        ToString::to_string,
                                    ),
                                    test,
                                )
                            }))
                        }
                    },
                )
            })
    }
}
