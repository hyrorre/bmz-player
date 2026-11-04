use std::cmp::Ordering;

use anyhow::{Context, Result, bail};

use super::VirtualChartFacts;

#[derive(Debug, Clone, PartialEq)]
pub(super) struct VirtualQuery {
    expression: Expr,
}

#[derive(Debug, Clone, PartialEq)]
enum Expr {
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    Not(Box<Expr>),
    Compare(Field, CompareOp, Literal),
    InList(Field, Vec<Literal>),
    InTimeRange(Field, TimeRangeExpr),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CompareOp {
    Eq,
    NotEq,
    Lt,
    LessEq,
    Gt,
    GreaterEq,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Field {
    Mode,
    Level,
    Density,
    PeakDensity,
    EndDensity,
    ScratchRate,
    LongNoteRate,
    Clear,
    ScoreRate,
    PlayCount,
    AddedAt,
    LampUpdatedAt,
    ScoreUpdatedAt,
}

impl Field {
    pub(super) fn parse(value: &str) -> Result<Self> {
        match value {
            "mode" => Ok(Self::Mode),
            "level" => Ok(Self::Level),
            "density" => Ok(Self::Density),
            "peak_density" => Ok(Self::PeakDensity),
            "end_density" => Ok(Self::EndDensity),
            "scratch_rate" => Ok(Self::ScratchRate),
            "long_note_rate" => Ok(Self::LongNoteRate),
            "clear" => Ok(Self::Clear),
            "score_rate" => Ok(Self::ScoreRate),
            "play_count" => Ok(Self::PlayCount),
            "added_at" => Ok(Self::AddedAt),
            "lamp_updated_at" => Ok(Self::LampUpdatedAt),
            "score_updated_at" => Ok(Self::ScoreUpdatedAt),
            _ => bail!("unknown virtual-folder field `{value}`"),
        }
    }

    fn accepts_literal(self, literal: &Literal) -> bool {
        match self {
            Self::Mode => matches!(literal, Literal::String(_)),
            Self::Level
            | Self::Density
            | Self::PeakDensity
            | Self::EndDensity
            | Self::ScratchRate
            | Self::LongNoteRate
            | Self::Clear
            | Self::ScoreRate
            | Self::PlayCount
            | Self::AddedAt => matches!(literal, Literal::Number(_)),
            Self::LampUpdatedAt | Self::ScoreUpdatedAt => false,
        }
    }

    fn accepts_time_range(self) -> bool {
        matches!(self, Self::AddedAt | Self::LampUpdatedAt | Self::ScoreUpdatedAt)
    }

    pub(super) fn can_order(self) -> bool {
        !matches!(self, Self::LampUpdatedAt | Self::ScoreUpdatedAt)
    }

    fn value<'facts>(self, facts: &VirtualChartFacts<'facts>) -> Option<Value<'facts>> {
        match self {
            Self::Mode => Some(Value::String(&facts.chart.mode)),
            Self::Level => facts.chart.play_level.parse::<f64>().ok().map(Value::Number),
            Self::Density => facts.analysis.map(|analysis| Value::Number(analysis.density)),
            Self::PeakDensity => {
                facts.analysis.map(|analysis| Value::Number(analysis.peak_density))
            }
            Self::EndDensity => facts.analysis.map(|analysis| Value::Number(analysis.end_density)),
            Self::ScratchRate => facts.analysis.map(|analysis| {
                let total = analysis.normal_notes.saturating_add(analysis.long_notes);
                let scratch = analysis.scratch_notes.saturating_add(analysis.long_scratch_notes);
                Value::Number(ratio(scratch, total))
            }),
            Self::LongNoteRate => facts.analysis.map(|analysis| {
                let total = analysis.normal_notes.saturating_add(analysis.long_notes);
                Value::Number(ratio(analysis.long_notes, total))
            }),
            Self::Clear => Some(Value::Number(
                facts
                    .score
                    .map(|score| bmz_core::clear::ClearType::rank_from_label(&score.clear_type))
                    .unwrap_or(0)
                    .into(),
            )),
            Self::ScoreRate => Some(Value::Number(facts.score.map_or(0.0, |score| {
                facts.chart.ex_score_percent(score.ex_score, facts.score_key.ln_policy)
            }))),
            Self::PlayCount => {
                Some(Value::Number(facts.score.map_or(0.0, |score| score.play_count.into())))
            }
            Self::AddedAt => Some(Value::Timestamp(facts.first_seen_at)),
            Self::LampUpdatedAt => Some(Value::Timestamps(&facts.update_times.lamp)),
            Self::ScoreUpdatedAt => Some(Value::Timestamps(&facts.update_times.score)),
        }
    }
}

fn ratio(part: u32, total: u32) -> f64 {
    if total == 0 { 0.0 } else { f64::from(part) / f64::from(total) }
}

#[derive(Debug, Clone, PartialEq)]
enum Literal {
    Number(f64),
    String(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TimeRangeExpr {
    LocalDay(usize),
    LocalDays(usize),
}

#[derive(Debug, Clone, Copy)]
enum Value<'a> {
    Number(f64),
    String(&'a str),
    Timestamp(i64),
    Timestamps(&'a [i64]),
}

impl VirtualQuery {
    pub(super) fn parse(source: &str) -> Result<Self> {
        let mut parser = Parser::new(source)?;
        let expression = parser.parse_expression()?;
        parser.expect_end()?;
        Ok(Self { expression })
    }

    pub(super) fn matches(&self, facts: &VirtualChartFacts<'_>, local_days: &[(i64, i64)]) -> bool {
        self.expression.matches(facts, local_days)
    }

    pub(super) fn required_local_days(&self) -> usize {
        self.expression.required_local_days()
    }
}

impl Expr {
    fn matches(&self, facts: &VirtualChartFacts<'_>, local_days: &[(i64, i64)]) -> bool {
        match self {
            Self::And(left, right) => {
                left.matches(facts, local_days) && right.matches(facts, local_days)
            }
            Self::Or(left, right) => {
                left.matches(facts, local_days) || right.matches(facts, local_days)
            }
            Self::Not(expr) => !expr.matches(facts, local_days),
            Self::Compare(field, op, literal) => {
                field.value(facts).is_some_and(|value| compare(value, *op, literal))
            }
            Self::InList(field, values) => field.value(facts).is_some_and(|value| {
                values.iter().any(|literal| compare(value, CompareOp::Eq, literal))
            }),
            Self::InTimeRange(field, range) => field.value(facts).is_some_and(|value| {
                let Some((start, end)) = time_range(*range, local_days) else {
                    return false;
                };
                match value {
                    Value::Timestamp(timestamp) => timestamp >= start && timestamp < end,
                    Value::Timestamps(timestamps) => {
                        timestamps.iter().any(|timestamp| *timestamp >= start && *timestamp < end)
                    }
                    Value::Number(_) | Value::String(_) => false,
                }
            }),
        }
    }

    fn required_local_days(&self) -> usize {
        match self {
            Self::And(left, right) | Self::Or(left, right) => {
                left.required_local_days().max(right.required_local_days())
            }
            Self::Not(expression) => expression.required_local_days(),
            Self::InTimeRange(_, TimeRangeExpr::LocalDay(index)) => index.saturating_add(1),
            Self::InTimeRange(_, TimeRangeExpr::LocalDays(count)) => *count,
            Self::Compare(..) | Self::InList(..) => 0,
        }
    }
}

fn compare(value: Value<'_>, op: CompareOp, literal: &Literal) -> bool {
    let ordering = match (value, literal) {
        (Value::Number(left), Literal::Number(right)) => left.partial_cmp(right),
        (Value::Timestamp(left), Literal::Number(right)) => (left as f64).partial_cmp(right),
        (Value::String(left), Literal::String(right)) => Some(left.cmp(right)),
        (Value::Timestamps(_), _) | (_, _) => None,
    };
    match op {
        CompareOp::Eq => ordering == Some(Ordering::Equal),
        CompareOp::NotEq => ordering.is_some_and(|value| value != Ordering::Equal),
        CompareOp::Lt => ordering == Some(Ordering::Less),
        CompareOp::LessEq => {
            matches!(ordering, Some(Ordering::Less | Ordering::Equal))
        }
        CompareOp::Gt => ordering == Some(Ordering::Greater),
        CompareOp::GreaterEq => {
            matches!(ordering, Some(Ordering::Greater | Ordering::Equal))
        }
    }
}

fn time_range(range: TimeRangeExpr, local_days: &[(i64, i64)]) -> Option<(i64, i64)> {
    match range {
        TimeRangeExpr::LocalDay(index) => local_days.get(index).copied(),
        TimeRangeExpr::LocalDays(count) if count > 0 => {
            let oldest = local_days.get(count - 1)?;
            let newest = local_days.first()?;
            Some((oldest.0, newest.1))
        }
        TimeRangeExpr::LocalDays(_) => None,
    }
}

pub(super) fn compare_for_order(
    field: Field,
    left: &VirtualChartFacts<'_>,
    right: &VirtualChartFacts<'_>,
) -> Ordering {
    match (field.value(left), field.value(right)) {
        (Some(Value::Number(left)), Some(Value::Number(right))) => {
            left.partial_cmp(&right).unwrap_or(Ordering::Equal)
        }
        (Some(Value::Timestamp(left)), Some(Value::Timestamp(right))) => left.cmp(&right),
        (Some(Value::String(left)), Some(Value::String(right))) => left.cmp(right),
        _ => Ordering::Equal,
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Identifier(String),
    Number(f64),
    String(String),
    And,
    Or,
    Not,
    Eq,
    NotEq,
    Lt,
    LessEq,
    Gt,
    GreaterEq,
    In,
    LeftParen,
    RightParen,
    LeftBracket,
    RightBracket,
    Comma,
    End,
}

struct Parser {
    tokens: Vec<Token>,
    position: usize,
}

impl Parser {
    fn new(source: &str) -> Result<Self> {
        Ok(Self { tokens: tokenize(source)?, position: 0 })
    }

    fn parse_expression(&mut self) -> Result<Expr> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Result<Expr> {
        let mut expression = self.parse_and()?;
        while self.consume(&Token::Or) {
            expression = Expr::Or(Box::new(expression), Box::new(self.parse_and()?));
        }
        Ok(expression)
    }

    fn parse_and(&mut self) -> Result<Expr> {
        let mut expression = self.parse_unary()?;
        while self.consume(&Token::And) {
            expression = Expr::And(Box::new(expression), Box::new(self.parse_unary()?));
        }
        Ok(expression)
    }

    fn parse_unary(&mut self) -> Result<Expr> {
        if self.consume(&Token::Not) {
            return Ok(Expr::Not(Box::new(self.parse_unary()?)));
        }
        if self.consume(&Token::LeftParen) {
            let expression = self.parse_expression()?;
            self.expect(&Token::RightParen)?;
            return Ok(expression);
        }
        self.parse_predicate()
    }

    fn parse_predicate(&mut self) -> Result<Expr> {
        let field_name = match self.next() {
            Token::Identifier(value) => value,
            token => bail!("expected field name, found {token:?}"),
        };
        let field = Field::parse(&field_name)?;
        if self.consume(&Token::In) {
            if self.consume(&Token::LeftBracket) {
                let mut values = Vec::new();
                if !self.consume(&Token::RightBracket) {
                    loop {
                        let literal = self.parse_literal()?;
                        validate_literal(field, &literal)?;
                        values.push(literal);
                        if self.consume(&Token::RightBracket) {
                            break;
                        }
                        self.expect(&Token::Comma)?;
                    }
                }
                return Ok(Expr::InList(field, values));
            }
            let function = match self.next() {
                Token::Identifier(value) => value,
                token => bail!("expected time-range function after `in`, found {token:?}"),
            };
            self.expect(&Token::LeftParen)?;
            let count = match self.next() {
                Token::Number(value) if value >= 0.0 && value.fract() == 0.0 => value as usize,
                token => bail!("expected non-negative integer, found {token:?}"),
            };
            self.expect(&Token::RightParen)?;
            let range = match function.as_str() {
                "local_day" => TimeRangeExpr::LocalDay(count),
                "local_days" => TimeRangeExpr::LocalDays(count),
                _ => bail!("unknown time-range function `{function}`"),
            };
            if !field.accepts_time_range() {
                bail!("field `{field_name}` cannot be matched against a time range");
            }
            return Ok(Expr::InTimeRange(field, range));
        }
        let op = match self.next() {
            Token::Eq => CompareOp::Eq,
            Token::NotEq => CompareOp::NotEq,
            Token::Lt => CompareOp::Lt,
            Token::LessEq => CompareOp::LessEq,
            Token::Gt => CompareOp::Gt,
            Token::GreaterEq => CompareOp::GreaterEq,
            token => bail!("expected comparison operator, found {token:?}"),
        };
        let literal = self.parse_literal()?;
        validate_literal(field, &literal)?;
        Ok(Expr::Compare(field, op, literal))
    }

    fn parse_literal(&mut self) -> Result<Literal> {
        match self.next() {
            Token::Number(value) => Ok(Literal::Number(value)),
            Token::String(value) | Token::Identifier(value) => Ok(Literal::String(value)),
            token => bail!("expected number or string, found {token:?}"),
        }
    }

    fn next(&mut self) -> Token {
        let token = self.tokens.get(self.position).cloned().unwrap_or(Token::End);
        self.position = self.position.saturating_add(1);
        token
    }

    fn peek(&self) -> &Token {
        self.tokens.get(self.position).unwrap_or(&Token::End)
    }

    fn consume(&mut self, expected: &Token) -> bool {
        if self.peek() == expected {
            self.position += 1;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, expected: &Token) -> Result<()> {
        let actual = self.next();
        if &actual == expected { Ok(()) } else { bail!("expected {expected:?}, found {actual:?}") }
    }

    fn expect_end(&self) -> Result<()> {
        if matches!(self.peek(), Token::End) {
            Ok(())
        } else {
            bail!("unexpected token {:?}", self.peek())
        }
    }
}

fn validate_literal(field: Field, literal: &Literal) -> Result<()> {
    if field.accepts_literal(literal) {
        Ok(())
    } else {
        bail!("literal type does not match field `{field:?}`")
    }
}

fn tokenize(source: &str) -> Result<Vec<Token>> {
    let chars: Vec<char> = source.chars().collect();
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        let character = chars[index];
        if character.is_whitespace() {
            index += 1;
            continue;
        }
        let two = chars.get(index + 1).map(|next| (character, *next));
        if let Some(token) = match two {
            Some(('&', '&')) => Some(Token::And),
            Some(('|', '|')) => Some(Token::Or),
            Some(('=', '=')) => Some(Token::Eq),
            Some(('!', '=')) => Some(Token::NotEq),
            Some(('<', '=')) => Some(Token::LessEq),
            Some(('>', '=')) => Some(Token::GreaterEq),
            _ => None,
        } {
            tokens.push(token);
            index += 2;
            continue;
        }
        let token = match character {
            '!' => Token::Not,
            '<' => Token::Lt,
            '>' => Token::Gt,
            '(' => Token::LeftParen,
            ')' => Token::RightParen,
            '[' => Token::LeftBracket,
            ']' => Token::RightBracket,
            ',' => Token::Comma,
            '"' | '\'' => {
                let quote = character;
                index += 1;
                let mut value = String::new();
                while index < chars.len() && chars[index] != quote {
                    if chars[index] == '\\' {
                        index += 1;
                        let escaped = chars.get(index).context("unterminated string escape")?;
                        value.push(match escaped {
                            'n' => '\n',
                            'r' => '\r',
                            't' => '\t',
                            other => *other,
                        });
                    } else {
                        value.push(chars[index]);
                    }
                    index += 1;
                }
                if chars.get(index) != Some(&quote) {
                    bail!("unterminated string literal");
                }
                Token::String(value)
            }
            '-' | '0'..='9' | '.' => {
                let start = index;
                index += 1;
                while index < chars.len()
                    && (chars[index].is_ascii_digit()
                        || matches!(chars[index], '.' | 'e' | 'E' | '+' | '-'))
                {
                    index += 1;
                }
                let text: String = chars[start..index].iter().collect();
                index -= 1;
                Token::Number(text.parse().with_context(|| format!("invalid number `{text}`"))?)
            }
            character if character.is_alphabetic() || character == '_' => {
                let start = index;
                index += 1;
                while index < chars.len()
                    && (chars[index].is_alphanumeric() || matches!(chars[index], '_' | '-'))
                {
                    index += 1;
                }
                let value: String = chars[start..index].iter().collect();
                index -= 1;
                if value == "in" { Token::In } else { Token::Identifier(value) }
            }
            _ => bail!("unexpected character `{character}`"),
        };
        tokens.push(token);
        index += 1;
    }
    tokens.push(Token::End);
    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_compound_one_line_query() {
        let query =
            VirtualQuery::parse("mode in [\"7K\", \"14K\"] && density >= 10 && density < 20")
                .unwrap();
        assert!(matches!(query.expression, Expr::And(_, _)));
    }

    #[test]
    fn rejects_unknown_field() {
        let error = VirtualQuery::parse("raw_sql == 1").unwrap_err().to_string();
        assert!(error.contains("unknown virtual-folder field"));
    }

    #[test]
    fn rejects_wrong_literal_type() {
        let error = VirtualQuery::parse("density == 'dense'").unwrap_err().to_string();
        assert!(error.contains("literal type"));
    }

    #[test]
    fn parses_local_day_range() {
        let query = VirtualQuery::parse("lamp_updated_at in local_day(12)").unwrap();
        assert_eq!(
            query.expression,
            Expr::InTimeRange(Field::LampUpdatedAt, TimeRangeExpr::LocalDay(12))
        );
    }
}
