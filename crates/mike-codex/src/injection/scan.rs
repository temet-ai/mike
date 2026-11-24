use std::io::{BufRead, BufReader, Read};

use super::{InjectionError, InjectionRefusal, InputShape, RequestInfo};

pub(super) enum Input {
    Array {
        close: u64,
        nonempty: bool,
        last_input_is_tool_result: bool,
    },
    String {
        start: u64,
        end: u64,
    },
}

impl Input {
    pub(super) fn info(&self) -> RequestInfo {
        match self {
            Self::Array {
                last_input_is_tool_result,
                ..
            } => RequestInfo {
                input: InputShape::Array,
                last_input_is_tool_result: *last_input_is_tool_result,
            },
            Self::String { .. } => RequestInfo {
                input: InputShape::String,
                last_input_is_tool_result: false,
            },
        }
    }
}

#[derive(PartialEq, Eq)]
enum Symbol {
    Input,
    Type,
    FunctionOutput,
    CustomOutput,
}

const SYMBOLS: [(&[u8], Symbol); 4] = [
    (b"input", Symbol::Input),
    (b"type", Symbol::Type),
    (b"function_call_output", Symbol::FunctionOutput),
    (b"custom_tool_call_output", Symbol::CustomOutput),
];

pub(super) fn locate(input: impl Read) -> Result<Result<Input, InjectionRefusal>, InjectionError> {
    let mut cursor = Cursor {
        reader: BufReader::new(input),
        position: 0,
    };
    cursor.whitespace()?;
    if cursor.next()? != Some(b'{') {
        return Ok(Err(InjectionRefusal::NotObject));
    }
    let mut selected = None;
    let mut matched = false;
    loop {
        cursor.whitespace()?;
        if cursor.peek()? == Some(b'}') {
            break;
        }
        cursor.required()?;
        let is_input = cursor.symbol()? == Some(Symbol::Input);
        cursor.whitespace()?;
        cursor.required()?;
        cursor.whitespace()?;
        if is_input && matched {
            return Ok(Err(InjectionRefusal::DuplicateInput));
        }
        if is_input {
            matched = true;
            selected = cursor.input_value()?;
        } else {
            cursor.skip_value()?;
        }
        cursor.whitespace()?;
        if cursor.next()? != Some(b',') {
            break;
        }
    }
    Ok(selected.ok_or(if matched {
        InjectionRefusal::UnsupportedInput
    } else {
        InjectionRefusal::MissingInput
    }))
}

struct Cursor<R> {
    reader: BufReader<R>,
    position: u64,
}

impl<R: Read> Cursor<R> {
    fn peek(&mut self) -> Result<Option<u8>, InjectionError> {
        Ok(self.reader.fill_buf()?.first().copied())
    }

    fn next(&mut self) -> Result<Option<u8>, InjectionError> {
        let byte = self.peek()?;
        if byte.is_some() {
            self.reader.consume(1);
            self.position += 1;
        }
        Ok(byte)
    }

    fn required(&mut self) -> Result<u8, InjectionError> {
        let byte = self.required_peek()?;
        self.next()?;
        Ok(byte)
    }

    fn required_peek(&mut self) -> Result<u8, InjectionError> {
        self.peek()?.ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "Responses request ended after JSON validation",
            )
            .into()
        })
    }

    fn whitespace(&mut self) -> Result<(), InjectionError> {
        while self.peek()?.is_some_and(|byte| byte.is_ascii_whitespace()) {
            self.next()?;
        }
        Ok(())
    }

    fn symbol(&mut self) -> Result<Option<Symbol>, InjectionError> {
        let mut index = 0;
        let mut matched = [true; SYMBOLS.len()];
        while let Some(character) = self.string_character()? {
            for (matches, (expected, _)) in matched.iter_mut().zip(&SYMBOLS) {
                *matches &= expected.get(index).copied().map(u32::from) == Some(character);
            }
            index = (index + 1).min(b"custom_tool_call_output".len() + 1);
        }
        Ok(matched
            .into_iter()
            .zip(SYMBOLS)
            .find_map(|(matches, (expected, symbol))| {
                (matches && index == expected.len()).then_some(symbol)
            }))
    }

    fn input_value(&mut self) -> Result<Option<Input>, InjectionError> {
        let start = self.position;
        match self.peek()? {
            Some(b'[') => {
                self.next()?;
                self.whitespace()?;
                let nonempty = self.peek()? != Some(b']');
                let last_input_is_tool_result = self.input_items()?;
                Ok(Some(Input::Array {
                    close: self.position - 1,
                    nonempty,
                    last_input_is_tool_result,
                }))
            }
            Some(b'"') => {
                self.next()?;
                self.string()?;
                Ok(Some(Input::String {
                    start,
                    end: self.position,
                }))
            }
            _ => {
                self.skip_value()?;
                Ok(None)
            }
        }
    }

    fn input_items(&mut self) -> Result<bool, InjectionError> {
        let mut last_tool_result = false;
        while self.required_peek()? != b']' {
            let start = self.position;
            last_tool_result = self.input_item()?;
            if self.position == start {
                return Err(InjectionError::InvalidString(
                    "input changed during inspection",
                ));
            }
            self.whitespace()?;
            if self.peek()? == Some(b',') {
                self.next()?;
                self.whitespace()?;
            }
        }
        self.required()?;
        Ok(last_tool_result)
    }

    fn input_item(&mut self) -> Result<bool, InjectionError> {
        if self.peek()? != Some(b'{') {
            self.skip_value()?;
            return Ok(false);
        }
        self.next()?;
        self.whitespace()?;
        let mut tool_result = false;
        while self.peek()? != Some(b'}') {
            self.required()?;
            let key = self.symbol()?;
            self.whitespace()?;
            self.required()?;
            self.whitespace()?;
            if key == Some(Symbol::Type) {
                tool_result = self.tool_result_type()?;
            } else {
                self.skip_value()?;
            }
            self.whitespace()?;
            if self.peek()? == Some(b',') {
                self.next()?;
                self.whitespace()?;
            }
        }
        self.required()?;
        Ok(tool_result)
    }

    fn tool_result_type(&mut self) -> Result<bool, InjectionError> {
        if self.peek()? == Some(b'"') {
            self.next()?;
            Ok(matches!(
                self.symbol()?,
                Some(Symbol::FunctionOutput | Symbol::CustomOutput)
            ))
        } else {
            self.skip_value()?;
            Ok(false)
        }
    }

    fn skip_value(&mut self) -> Result<(), InjectionError> {
        match self.peek()? {
            Some(b'"') => {
                self.next()?;
                self.string()
            }
            Some(b'[' | b'{') => {
                self.next()?;
                self.compound()
            }
            _ => {
                while self.peek()?.is_some_and(|byte| {
                    !byte.is_ascii_whitespace() && !matches!(byte, b',' | b']' | b'}')
                }) {
                    self.next()?;
                }
                Ok(())
            }
        }
    }

    fn compound(&mut self) -> Result<(), InjectionError> {
        let mut depth = 1;
        while depth > 0 {
            match self.required()? {
                b'"' => self.string()?,
                b'[' | b'{' => depth += 1,
                b']' | b'}' => depth -= 1,
                _ => {}
            }
        }
        Ok(())
    }

    fn string(&mut self) -> Result<(), InjectionError> {
        while self.string_character()?.is_some() {}
        Ok(())
    }

    fn string_character(&mut self) -> Result<Option<u32>, InjectionError> {
        match self.required()? {
            b'"' => Ok(None),
            b'\\' => self.escape().map(Some),
            byte @ 0..=31 => Err(InjectionError::InvalidString(if byte == 0 {
                "NUL must be escaped"
            } else {
                "unescaped control character"
            })),
            byte @ 32..=127 => Ok(Some(u32::from(byte))),
            byte => self.utf8(byte).map(Some),
        }
    }

    fn escape(&mut self) -> Result<u32, InjectionError> {
        match self.required()? {
            byte @ (b'"' | b'\\' | b'/') => Ok(u32::from(byte)),
            b'b' => Ok(8),
            b'f' => Ok(12),
            b'n' => Ok(10),
            b'r' => Ok(13),
            b't' => Ok(9),
            b'u' => self.unicode_escape(),
            _ => Err(InjectionError::InvalidString("unknown escape")),
        }
    }

    fn unicode_escape(&mut self) -> Result<u32, InjectionError> {
        let high = self.hex()?;
        match high {
            0xd800..=0xdbff => {
                if self.required()? != b'\\' || self.required()? != b'u' {
                    return Err(InjectionError::InvalidString("unpaired high surrogate"));
                }
                let low = self.hex()?;
                if !(0xdc00..=0xdfff).contains(&low) {
                    return Err(InjectionError::InvalidString("invalid low surrogate"));
                }
                Ok(0x10000 + ((high - 0xd800) << 10) + low - 0xdc00)
            }
            0xdc00..=0xdfff => Err(InjectionError::InvalidString("unpaired low surrogate")),
            _ => Ok(high),
        }
    }

    fn hex(&mut self) -> Result<u32, InjectionError> {
        let mut value = 0;
        for _ in 0..4 {
            let digit = char::from(self.required()?)
                .to_digit(16)
                .ok_or(InjectionError::InvalidString("invalid unicode escape"))?;
            value = value * 16 + digit;
        }
        Ok(value)
    }

    fn utf8(&mut self, first: u8) -> Result<u32, InjectionError> {
        let width = match first {
            0xc2..=0xdf => 2,
            0xe0..=0xef => 3,
            0xf0..=0xf4 => 4,
            _ => return Err(InjectionError::InvalidString("invalid UTF-8 leading byte")),
        };
        let mut bytes = [first, 0, 0, 0];
        for byte in &mut bytes[1..width] {
            *byte = self.required()?;
        }
        std::str::from_utf8(&bytes[..width])?
            .chars()
            .next()
            .map(u32::from)
            .ok_or(InjectionError::InvalidString("empty UTF-8 character"))
    }
}
