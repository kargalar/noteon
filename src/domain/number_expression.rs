//! Arithmetic input for number properties, without executing code or accepting non-finite values.

const MAX_EXPRESSION_DEPTH: usize = 64;

pub fn evaluate(input: &str) -> Option<String> {
    let normalized = input
        .replace(',', ".")
        .replace('×', "*")
        .replace('÷', "/")
        .replace('−', "-");
    let mut parser = Parser {
        remaining: normalized.trim(),
    };
    let value = parser.expression(0, 0)?;
    parser.skip_whitespace();
    (parser.remaining.is_empty() && value.is_finite()).then(|| {
        if value == 0.0 {
            "0".to_owned()
        } else {
            value.to_string()
        }
    })
}

struct Parser<'a> {
    remaining: &'a str,
}

impl Parser<'_> {
    fn skip_whitespace(&mut self) {
        self.remaining = self.remaining.trim_start();
    }

    fn expression(&mut self, minimum_precedence: u8, depth: usize) -> Option<f64> {
        if depth >= MAX_EXPRESSION_DEPTH {
            return None;
        }
        let mut value = self.operand(depth)?;
        loop {
            self.skip_whitespace();
            let Some(operator) = self.remaining.bytes().next() else {
                break;
            };
            let precedence = match operator {
                b'+' | b'-' => 1,
                b'*' | b'/' => 2,
                _ => break,
            };
            if precedence < minimum_precedence {
                break;
            }
            self.remaining = &self.remaining[1..];
            let right = self.expression(precedence + 1, depth + 1)?;
            value = match operator {
                b'+' => value + right,
                b'-' => value - right,
                b'*' => value * right,
                b'/' if right != 0.0 => value / right,
                _ => return None,
            };
            if !value.is_finite() {
                return None;
            }
        }
        Some(value)
    }

    fn operand(&mut self, depth: usize) -> Option<f64> {
        let mut negative = false;
        loop {
            self.skip_whitespace();
            match self.remaining.bytes().next()? {
                b'+' => self.remaining = &self.remaining[1..],
                b'-' => {
                    negative = !negative;
                    self.remaining = &self.remaining[1..];
                }
                _ => break,
            }
        }
        let value = if self.remaining.starts_with('(') {
            self.remaining = &self.remaining[1..];
            let value = self.expression(0, depth + 1)?;
            self.skip_whitespace();
            self.remaining = self.remaining.strip_prefix(')')?;
            value
        } else {
            self.number()?
        };
        Some(if negative { -value } else { value })
    }

    fn number(&mut self) -> Option<f64> {
        let mut length = 0;
        let mut exponent = false;
        for (index, byte) in self.remaining.bytes().enumerate() {
            match byte {
                b'0'..=b'9' | b'.' => {}
                b'e' | b'E' if !exponent => exponent = true,
                b'+' | b'-'
                    if index > 0 && matches!(self.remaining.as_bytes()[index - 1], b'e' | b'E') => {
                }
                _ => break,
            }
            length = index + 1;
        }
        let value: f64 = self.remaining[..length].parse().ok()?;
        self.remaining = &self.remaining[length..];
        value.is_finite().then_some(value)
    }
}

#[cfg(test)]
mod tests {
    use super::evaluate;

    #[test]
    fn evaluates_arithmetic_with_precedence_and_decimal_input() {
        for (input, expected) in [
            ("500+20", "520"),
            ("500-20", "480"),
            ("500*20", "10000"),
            ("500/20", "25"),
            ("20 / 5 / 2", "2"),
            ("20-5-2", "13"),
            ("2+3*4", "14"),
            ("(2+3)*4", "20"),
            ("-2 * -3", "6"),
            ("1,5 + 2.5", "4"),
            ("2 × 3 ÷ 4", "1.5"),
            ("1e3 + .5", "1000.5"),
            ("1/8", "0.125"),
            ("-0", "0"),
        ] {
            assert_eq!(evaluate(input).as_deref(), Some(expected), "{input}");
        }
    }

    #[test]
    fn rejects_invalid_expressions_and_non_finite_results() {
        for input in [
            "", "500+", "2**3", "1/0", "0/0", "NaN", "inf", "1e999", "1e308*10", "(2+3", "2+3)",
            "2 3", "2a", "1.2.3",
        ] {
            assert_eq!(evaluate(input), None, "{input}");
        }
        assert_eq!(
            evaluate(&format!("{}1{}", "(".repeat(100), ")".repeat(100))),
            None
        );
    }
}
