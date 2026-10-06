//! Calculated fields: `"compute": "qty * price"`.
//!
//! The expression uses the record's own whole-number and decimal fields, numbers, `+`, `-`, `*`
//! and parentheses. There is no division: it cannot be exact, so a plugin that needs one rounds
//! it itself. The kernel turns the expression into integer arithmetic on the stored whole units
//! (decimals are stored as whole units of their smallest digit, see [`super::decimal`]), so the
//! result is exact, and rounds half away from zero only when it has more digits than the field.
//! A missing number counts as zero.

use std::collections::BTreeSet;

/// Longest expression, in characters.
const MAX_LENGTH: usize = 200;
/// Deepest nesting of parentheses.
const MAX_DEPTH: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Number(String),
    Name(String),
    Plus,
    Minus,
    Star,
    Open,
    Close,
    Dot,
}

/// A parsed expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    /// A literal: whole units and the digits after the point it was written with.
    Number { units: i64, scale: u32 },
    Field(String),
    Neg(Box<Expr>),
    Add(Box<Expr>, Box<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    /// `sum(lines.amount)` or `count(lines)`: over the rows of a child field.
    Rollup(Rollup),
}

/// A total over the rows of a child field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rollup {
    /// The child field of the model.
    pub child: String,
    /// The field of the rows that is added up; `None` counts the rows.
    pub source: Option<String>,
}

/// A field as the expression sees it: the stored column, and its digits after the point.
#[derive(Debug, Clone)]
pub struct Operand {
    pub column: String,
    pub scale: u32,
}

fn tokens(text: &str) -> Result<Vec<Token>, String> {
    if text.trim().is_empty() || text.chars().count() > MAX_LENGTH {
        return Err(format!("must be 1 to {MAX_LENGTH} characters"));
    }
    let mut out = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            ' ' => i += 1,
            '+' => {
                out.push(Token::Plus);
                i += 1;
            }
            '-' => {
                out.push(Token::Minus);
                i += 1;
            }
            '*' => {
                out.push(Token::Star);
                i += 1;
            }
            '.' => {
                out.push(Token::Dot);
                i += 1;
            }
            '(' => {
                out.push(Token::Open);
                i += 1;
            }
            ')' => {
                out.push(Token::Close);
                i += 1;
            }
            '/' | '%' => return Err("has no division: it cannot be exact, so round it in the plugin".into()),
            c if c.is_ascii_digit() => {
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                    i += 1;
                }
                out.push(Token::Number(chars[start..i].iter().collect()));
            }
            c if c.is_ascii_lowercase() => {
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_lowercase() || chars[i].is_ascii_digit() || chars[i] == '_') {
                    i += 1;
                }
                out.push(Token::Name(chars[start..i].iter().collect()));
            }
            other => return Err(format!("does not understand `{other}`")),
        }
    }
    Ok(out)
}

struct Parser {
    tokens: Vec<Token>,
    at: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.at)
    }

    fn sum(&mut self, depth: usize) -> Result<Expr, String> {
        let mut left = self.product(depth)?;
        while let Some(token) = self.peek() {
            let add = match token {
                Token::Plus => true,
                Token::Minus => false,
                _ => break,
            };
            self.at += 1;
            let right = self.product(depth)?;
            left = if add { Expr::Add(Box::new(left), Box::new(right)) } else { Expr::Sub(Box::new(left), Box::new(right)) };
        }
        Ok(left)
    }

    fn product(&mut self, depth: usize) -> Result<Expr, String> {
        let mut left = self.unary(depth)?;
        while self.peek() == Some(&Token::Star) {
            self.at += 1;
            let right = self.unary(depth)?;
            left = Expr::Mul(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn unary(&mut self, depth: usize) -> Result<Expr, String> {
        if self.peek() == Some(&Token::Minus) {
            self.at += 1;
            return Ok(Expr::Neg(Box::new(self.unary(depth)?)));
        }
        self.atom(depth)
    }

    fn atom(&mut self, depth: usize) -> Result<Expr, String> {
        let token = self.peek().cloned().ok_or("ends too soon")?;
        self.at += 1;
        match token {
            Token::Number(text) => {
                let value = super::decimal::to_scaled(&serde_json::Value::String(text.clone()), 9)
                    .map_err(|_| format!("has a bad number `{text}`"))?;
                let scale = text.split_once('.').map_or(0, |(_, fraction)| fraction.len() as u32);
                // `to_scaled` used scale 9; drop the padding zeros so the literal keeps its own scale.
                let units = value / 10_i64.pow(9 - scale.min(9));
                Ok(Expr::Number { units, scale })
            }
            Token::Name(name) if (name == "sum" || name == "count") && self.peek() == Some(&Token::Open) => {
                self.at += 1;
                let Some(Token::Name(child)) = self.peek().cloned() else {
                    return Err(format!("`{name}(` needs the child field's name"));
                };
                self.at += 1;
                let source = if name == "sum" {
                    if self.peek() != Some(&Token::Dot) {
                        return Err("`sum(` needs `child.field`, such as `sum(lines.amount)`".into());
                    }
                    self.at += 1;
                    let Some(Token::Name(source)) = self.peek().cloned() else {
                        return Err("`sum(child.` needs a field name".into());
                    };
                    self.at += 1;
                    Some(source)
                } else {
                    None
                };
                if self.peek() != Some(&Token::Close) {
                    return Err(format!("`{name}(` is not closed"));
                }
                self.at += 1;
                Ok(Expr::Rollup(Rollup { child, source }))
            }
            Token::Name(name) => Ok(Expr::Field(name)),
            Token::Open => {
                if depth >= MAX_DEPTH {
                    return Err("is nested too deeply".into());
                }
                let inner = self.sum(depth + 1)?;
                if self.peek() != Some(&Token::Close) {
                    return Err("has a `(` without a `)`".into());
                }
                self.at += 1;
                Ok(inner)
            }
            other => Err(format!("did not expect {other:?} here")),
        }
    }
}

impl Expr {
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut parser = Parser { tokens: tokens(text)?, at: 0 };
        let expr = parser.sum(0)?;
        if parser.at != parser.tokens.len() {
            return Err("has something left over after the end".into());
        }
        Ok(expr)
    }

    /// The field names the expression uses.
    pub fn fields(&self) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        self.collect(&mut out);
        out
    }

    /// The roll-ups the expression uses.
    pub fn rollups(&self) -> Vec<Rollup> {
        let mut out = Vec::new();
        self.collect_rollups(&mut out);
        out
    }

    fn collect_rollups(&self, out: &mut Vec<Rollup>) {
        match self {
            Self::Rollup(rollup) => out.push(rollup.clone()),
            Self::Number { .. } | Self::Field(_) => {}
            Self::Neg(inner) => inner.collect_rollups(out),
            Self::Add(a, b) | Self::Sub(a, b) | Self::Mul(a, b) => {
                a.collect_rollups(out);
                b.collect_rollups(out);
            }
        }
    }

    fn collect(&self, out: &mut BTreeSet<String>) {
        match self {
            Self::Number { .. } | Self::Rollup(_) => {}
            Self::Field(name) => {
                out.insert(name.clone());
            }
            Self::Neg(inner) => inner.collect(out),
            Self::Add(a, b) | Self::Sub(a, b) | Self::Mul(a, b) => {
                a.collect(out);
                b.collect(out);
            }
        }
    }

    /// SurrealQL for the value in whole units of `target` digits after the point, reading the
    /// record's columns by name (inside `UPDATE … SET`) or from `row` (such as `$rows[0]`).
    /// `operand` resolves a field name.
    pub fn to_sql(
        &self,
        row: &str,
        target: u32,
        operand: &dyn Fn(&str) -> Option<Operand>,
        rollup: &dyn Fn(&Rollup) -> Option<(String, u32)>,
    ) -> Result<String, String> {
        let (sql, scale) = self.units(row, operand, rollup)?;
        Ok(match scale.cmp(&target) {
            std::cmp::Ordering::Equal => sql,
            std::cmp::Ordering::Less => format!("({sql} * {})", 10_i64.pow(target - scale)),
            // Round half away from zero. A whole unit divided by a power of ten is exact in a
            // float for the sizes money and quantities have, including a tie such as 12.5.
            std::cmp::Ordering::Greater => format!("<int>math::round(<float>{sql} / {})", 10_i64.pow(scale - target)),
        })
    }

    fn units(
        &self,
        row: &str,
        operand: &dyn Fn(&str) -> Option<Operand>,
        rollup: &dyn Fn(&Rollup) -> Option<(String, u32)>,
    ) -> Result<(String, u32), String> {
        Ok(match self {
            Self::Rollup(total) => rollup(total).ok_or_else(|| format!("rolls up `{}`, which is not available here", total.child))?,
            Self::Number { units, scale } => (format!("{units}"), *scale),
            Self::Field(name) => {
                let operand = operand(name).ok_or_else(|| format!("uses `{name}`, which is not a whole-number or decimal field"))?;
                let column = if row.is_empty() { operand.column } else { format!("{row}.{}", operand.column) };
                (format!("({column} ?? 0)"), operand.scale)
            }
            Self::Neg(inner) => {
                let (sql, scale) = inner.units(row, operand, rollup)?;
                (format!("(-{sql})"), scale)
            }
            Self::Mul(a, b) => {
                let ((a, sa), (b, sb)) = (a.units(row, operand, rollup)?, b.units(row, operand, rollup)?);
                if sa + sb > 18 {
                    return Err("multiplies numbers with too many digits after the point".into());
                }
                (format!("({a} * {b})"), sa + sb)
            }
            Self::Add(a, b) | Self::Sub(a, b) => {
                let ((a, sa), (b, sb)) = (a.units(row, operand, rollup)?, b.units(row, operand, rollup)?);
                let scale = sa.max(sb);
                let a = if sa < scale { format!("({a} * {})", 10_i64.pow(scale - sa)) } else { a };
                let b = if sb < scale { format!("({b} * {})", 10_i64.pow(scale - sb)) } else { b };
                let symbol = if matches!(self, Self::Add(..)) { "+" } else { "-" };
                (format!("({a} {symbol} {b})"), scale)
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn operand(name: &str) -> Option<Operand> {
        match name {
            "qty" => Some(Operand { column: "fld_qty".into(), scale: 0 }),
            "price" => Some(Operand { column: "fld_price".into(), scale: 2 }),
            _ => None,
        }
    }

    #[test]
    fn precedence_and_fields() -> Result<(), String> {
        let expr = Expr::parse("qty * price + 1.5 - -2")?;
        assert_eq!(expr.fields().into_iter().collect::<Vec<_>>(), vec!["price", "qty"]);
        let sql = expr.to_sql("$rows[0]", 2, &operand, &|_| None)?;
        // qty (scale 0) * price (scale 2) is scale 2; 1.5 is scaled up to 150; 2 to 200.
        assert!(sql.contains("($rows[0].fld_qty ?? 0) * ($rows[0].fld_price ?? 0)"), "{sql}");
        assert!(sql.contains("(15 * 10)"), "{sql}");
        Ok(())
    }

    #[test]
    fn a_result_with_more_digits_than_the_field_is_rounded() -> Result<(), String> {
        let sql = Expr::parse("price * price")?.to_sql("$rows[0]", 2, &operand, &|_| None)?;
        assert!(sql.starts_with("<int>math::round(<float>"), "{sql}");
        assert!(sql.ends_with("/ 100)"), "{sql}");
        Ok(())
    }

    #[test]
    fn nonsense_is_refused() {
        for bad in ["sum(lines)", "sum(lines.)", "count(lines.x)", "sum lines.x", "", "qty /", "qty / 2", "qty *", "(qty", "qty qty", "qty $ 1", "1..2", "((((((((((qty))))))))))"] {
            assert!(Expr::parse(bad).is_err(), "{bad}");
        }
        assert!(Expr::parse("nope + 1").and_then(|e| e.to_sql("$r", 0, &operand, &|_| None)).is_err());
    }
}

#[cfg(test)]
mod rollup_tests {
    use super::*;

    #[test]
    fn totals_over_child_rows_are_parsed() -> Result<(), String> {
        let expr = Expr::parse("sum(lines.amount) - discount + count(lines)")?;
        let rollups = expr.rollups();
        assert_eq!(rollups.len(), 2);
        assert_eq!(rollups[0], Rollup { child: "lines".into(), source: Some("amount".into()) });
        assert_eq!(rollups[1], Rollup { child: "lines".into(), source: None });
        assert_eq!(expr.fields().into_iter().collect::<Vec<_>>(), vec!["discount"]);
        Ok(())
    }
}
