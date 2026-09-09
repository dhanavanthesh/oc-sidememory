use std::cmp::Ordering;

use thiserror::Error;

use super::limits::RuntimeLimits;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CanonicalNumber {
    negative: bool,
    coefficient: Box<str>,
    exponent: Box<str>,
}

impl CanonicalNumber {
    pub fn parse(bytes: &[u8], limits: &RuntimeLimits) -> Result<Self, NumberError> {
        if bytes.len() > limits.max_value_bytes {
            return Err(NumberError::LimitExceeded {
                limit_name: "max_value_bytes",
                limit_value: limits.max_value_bytes,
                requested: bytes.len(),
            });
        }
        let parts = NumberParts::parse(bytes, limits)?;
        let coefficient_len = parts
            .integer
            .len()
            .checked_add(parts.fraction.len())
            .ok_or(NumberError::Allocation {
                context: "number coefficient",
                requested: usize::MAX,
            })?;
        let mut coefficient = Vec::new();
        coefficient
            .try_reserve_exact(coefficient_len)
            .map_err(|_| NumberError::Allocation {
                context: "number coefficient",
                requested: coefficient_len,
            })?;
        coefficient.extend_from_slice(parts.integer);
        coefficient.extend_from_slice(parts.fraction);

        let first_nonzero = coefficient.iter().position(|digit| *digit != b'0');
        let Some(first_nonzero) = first_nonzero else {
            return Ok(Self {
                negative: false,
                coefficient: "0".into(),
                exponent: "0".into(),
            });
        };
        coefficient.drain(..first_nonzero);
        let trailing = coefficient
            .iter()
            .rev()
            .take_while(|digit| **digit == b'0')
            .count();
        coefficient.truncate(coefficient.len() - trailing);

        let mut exponent = SignedDecimal::parse(parts.exponent_negative, parts.exponent_digits)?;
        exponent.add_small_signed(-(parts.fraction.len() as i128))?;
        exponent.add_small_signed(trailing as i128)?;

        let coefficient = String::from_utf8(coefficient)
            .map_err(|_| NumberError::InvalidGrammar)?
            .into_boxed_str();
        Ok(Self {
            negative: parts.negative,
            coefficient,
            exponent: exponent.to_boxed_str()?,
        })
    }

    pub fn is_negative(&self) -> bool {
        self.negative
    }

    pub fn coefficient(&self) -> &str {
        &self.coefficient
    }

    pub fn exponent(&self) -> &str {
        &self.exponent
    }
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum NumberError {
    #[error("invalid JSON number grammar")]
    InvalidGrammar,
    #[error(
        "number resource limit exceeded: {limit_name} is {limit_value}, requested {requested}"
    )]
    LimitExceeded {
        limit_name: &'static str,
        limit_value: usize,
        requested: usize,
    },
    #[error("allocation failed while reserving {requested} bytes for {context}")]
    Allocation {
        context: &'static str,
        requested: usize,
    },
}

struct NumberParts<'a> {
    negative: bool,
    integer: &'a [u8],
    fraction: &'a [u8],
    exponent_negative: bool,
    exponent_digits: &'a [u8],
}

impl<'a> NumberParts<'a> {
    fn parse(bytes: &'a [u8], limits: &RuntimeLimits) -> Result<Self, NumberError> {
        let mut index = 0;
        let negative = bytes.first() == Some(&b'-');
        index += usize::from(negative);
        let integer_start = index;
        match bytes.get(index) {
            Some(b'0') => index += 1,
            Some(b'1'..=b'9') => {
                index += 1;
                while matches!(bytes.get(index), Some(b'0'..=b'9')) {
                    index += 1;
                }
            }
            _ => return Err(NumberError::InvalidGrammar),
        }
        let integer = &bytes[integer_start..index];
        if integer == b"0" && matches!(bytes.get(index), Some(b'0'..=b'9')) {
            return Err(NumberError::InvalidGrammar);
        }

        let mut fraction = &bytes[0..0];
        if bytes.get(index) == Some(&b'.') {
            index += 1;
            let start = index;
            while matches!(bytes.get(index), Some(b'0'..=b'9')) {
                index += 1;
            }
            if start == index {
                return Err(NumberError::InvalidGrammar);
            }
            fraction = &bytes[start..index];
        }

        let mut exponent_negative = false;
        let mut exponent_digits = &bytes[0..0];
        if matches!(bytes.get(index), Some(b'e' | b'E')) {
            index += 1;
            match bytes.get(index) {
                Some(b'-') => {
                    exponent_negative = true;
                    index += 1;
                }
                Some(b'+') => index += 1,
                _ => {}
            }
            let start = index;
            while matches!(bytes.get(index), Some(b'0'..=b'9')) {
                index += 1;
            }
            if start == index {
                return Err(NumberError::InvalidGrammar);
            }
            exponent_digits = &bytes[start..index];
        }
        if index != bytes.len() {
            return Err(NumberError::InvalidGrammar);
        }
        let number_digits =
            integer
                .len()
                .checked_add(fraction.len())
                .ok_or(NumberError::LimitExceeded {
                    limit_name: "max_number_digits",
                    limit_value: limits.max_number_digits,
                    requested: usize::MAX,
                })?;
        if number_digits > limits.max_number_digits {
            return Err(NumberError::LimitExceeded {
                limit_name: "max_number_digits",
                limit_value: limits.max_number_digits,
                requested: number_digits,
            });
        }
        if exponent_digits.len() > limits.max_exponent_digits {
            return Err(NumberError::LimitExceeded {
                limit_name: "max_exponent_digits",
                limit_value: limits.max_exponent_digits,
                requested: exponent_digits.len(),
            });
        }
        Ok(Self {
            negative,
            integer,
            fraction,
            exponent_negative,
            exponent_digits,
        })
    }
}

#[derive(Clone, Debug)]
struct SignedDecimal {
    negative: bool,
    digits: Vec<u8>,
}

impl SignedDecimal {
    fn parse(negative: bool, digits: &[u8]) -> Result<Self, NumberError> {
        let start = digits
            .iter()
            .position(|digit| *digit != b'0')
            .unwrap_or(digits.len());
        if start == digits.len() {
            Ok(Self {
                negative: false,
                digits: vec![0],
            })
        } else {
            let requested = digits.len() - start;
            let mut parsed = Vec::new();
            parsed
                .try_reserve_exact(requested)
                .map_err(|_| NumberError::Allocation {
                    context: "number exponent",
                    requested,
                })?;
            parsed.extend(digits[start..].iter().map(|digit| digit - b'0'));
            Ok(Self {
                negative,
                digits: parsed,
            })
        }
    }

    fn add_small_signed(&mut self, value: i128) -> Result<(), NumberError> {
        if value == 0 {
            return Ok(());
        }
        let other_negative = value < 0;
        let other = value.unsigned_abs().to_string();
        let mut other_digits = Vec::new();
        other_digits
            .try_reserve_exact(other.len())
            .map_err(|_| NumberError::Allocation {
                context: "number exponent adjustment",
                requested: other.len(),
            })?;
        other_digits.extend(other.bytes().map(|digit| digit - b'0'));
        if self.negative == other_negative {
            add_magnitude(&mut self.digits, &other_digits)?;
            return Ok(());
        }
        match compare_magnitude(&self.digits, &other_digits) {
            Ordering::Greater => subtract_magnitude(&mut self.digits, &other_digits),
            Ordering::Equal => {
                self.digits.clear();
                self.digits.push(0);
                self.negative = false;
            }
            Ordering::Less => {
                let mut result = other_digits;
                subtract_magnitude(&mut result, &self.digits);
                self.digits = result;
                self.negative = other_negative;
            }
        }
        Ok(())
    }

    fn to_boxed_str(&self) -> Result<Box<str>, NumberError> {
        let requested = self
            .digits
            .len()
            .checked_add(usize::from(self.negative))
            .ok_or(NumberError::Allocation {
                context: "number exponent string",
                requested: usize::MAX,
            })?;
        let mut output = String::new();
        output
            .try_reserve_exact(requested)
            .map_err(|_| NumberError::Allocation {
                context: "number exponent string",
                requested,
            })?;
        if self.negative {
            output.push('-');
        }
        output.extend(self.digits.iter().map(|digit| char::from(b'0' + digit)));
        Ok(output.into_boxed_str())
    }
}

fn compare_magnitude(left: &[u8], right: &[u8]) -> Ordering {
    left.len().cmp(&right.len()).then_with(|| left.cmp(right))
}

fn add_magnitude(left: &mut Vec<u8>, right: &[u8]) -> Result<(), NumberError> {
    let width = left.len().max(right.len());
    let reserve =
        width
            .saturating_sub(left.len())
            .checked_add(1)
            .ok_or(NumberError::Allocation {
                context: "number exponent addition",
                requested: usize::MAX,
            })?;
    left.try_reserve(reserve)
        .map_err(|_| NumberError::Allocation {
            context: "number exponent addition",
            requested: reserve,
        })?;
    if left.len() < width {
        left.splice(0..0, std::iter::repeat_n(0, width - left.len()));
    }
    let mut carry = 0;
    for offset in 0..width {
        let li = width - 1 - offset;
        let ri = right
            .len()
            .checked_sub(1 + offset)
            .map(|index| right[index])
            .unwrap_or(0);
        let sum = left[li] + ri + carry;
        left[li] = sum % 10;
        carry = sum / 10;
    }
    if carry != 0 {
        left.insert(0, carry);
    }
    Ok(())
}

fn subtract_magnitude(left: &mut Vec<u8>, right: &[u8]) {
    let mut borrow = 0_i16;
    for offset in 0..left.len() {
        let li = left.len() - 1 - offset;
        let ri = right
            .len()
            .checked_sub(1 + offset)
            .map(|index| right[index])
            .unwrap_or(0);
        let mut value = i16::from(left[li]) - i16::from(ri) - borrow;
        if value < 0 {
            value += 10;
            borrow = 1;
        } else {
            borrow = 0;
        }
        left[li] = value as u8;
    }
    let first = left
        .iter()
        .position(|digit| *digit != 0)
        .unwrap_or(left.len() - 1);
    left.drain(..first);
}
