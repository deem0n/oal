use crate::span::Span;
use chumsky::{prelude::*, Stream};
use std::fmt::{Debug, Display, Formatter};
use std::hash::Hash;
use string_interner::{DefaultSymbol, StringInterner};

pub type Symbol = DefaultSymbol;

pub trait Interner {
    fn register<T: AsRef<str>>(&mut self, s: T) -> Symbol;
    fn resolve(&self, sym: Symbol) -> &str;
}

pub trait Intern {
    fn copy<I: Interner>(&self, from: &I, to: &mut I) -> Self;
    fn as_str<'a, I: Interner>(&'a self, from: &'a I) -> &'a str;
}

pub trait Lexeme: Clone + PartialEq + Eq + Hash + Debug {
    type Kind: Copy + Clone + PartialEq + Eq + Hash + Debug;
    type Value: Debug + Intern;

    fn new(kind: Self::Kind, value: Self::Value) -> Self;
    fn kind(&self) -> Self::Kind;
    fn value(&self) -> &Self::Value;
    fn is_trivia(&self) -> bool;
    fn internalize<I: Interner>(self, i: &mut I) -> Self;
}

pub type TokenIdx = generational_token_list::ItemToken;

pub type TokenSpan<L> = (L, Span);

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct TokenAlias<L: Lexeme>(L::Kind, TokenIdx);

impl<L: Lexeme> Copy for TokenAlias<L> {}

impl<L: Lexeme> TokenAlias<L> {
    pub fn new(kind: L::Kind, idx: TokenIdx) -> Self {
        TokenAlias(kind, idx)
    }

    pub fn kind(&self) -> L::Kind {
        self.0
    }

    pub fn index(&self) -> TokenIdx {
        self.1
    }
}

impl<L: Lexeme> Display for TokenAlias<L> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:#?}", self.0)
    }
}

type ListArena<L> = generational_token_list::GenerationalTokenList<(L, Span)>;

#[derive(Debug)]
pub struct TokenList<L: Lexeme> {
    list: ListArena<L>,
    dict: StringInterner,
}

impl<L: Lexeme> Default for TokenList<L> {
    fn default() -> Self {
        TokenList {
            list: ListArena::default(),
            dict: StringInterner::default(),
        }
    }
}

impl<L: Lexeme> Interner for TokenList<L> {
    fn register<T: AsRef<str>>(&mut self, s: T) -> Symbol {
        self.dict.get_or_intern(s)
    }

    fn resolve(&self, sym: Symbol) -> &str {
        self.dict.resolve(sym).unwrap()
    }
}

impl<L> TokenList<L>
where
    L: Lexeme,
{
    pub fn get(&self, id: TokenIdx) -> &TokenSpan<L> {
        self.list.get(id).unwrap()
    }

    pub fn push(&mut self, t: TokenSpan<L>) -> TokenIdx {
        self.list.push_back(t)
    }

    pub fn len(&self) -> usize {
        self.list.tail().map_or(0, |(_, r)| r.end() + 1)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[allow(clippy::needless_lifetimes)]
    pub fn stream<'a>(
        &'a self,
    ) -> Stream<TokenAlias<L>, Span, impl Iterator<Item = (TokenAlias<L>, Span)> + 'a> {
        let len = self.len();
        // Prepare the parser iterator by ignoring trivia tokens and replacing values with indices.
        let iter = self
            .list
            .iter_with_tokens()
            .filter_map(move |(index, (token, span))| {
                if token.is_trivia() {
                    None
                } else {
                    Some((TokenAlias::new(token.kind(), index), *span))
                }
            });
        Stream::from_iter(Span::new(len..len + 1), iter)
    }
}

/// The tokenizer error type.
pub type ParserError = Simple<char, Span>;

/// Parse a string of characters, yielding a list of tokens.
pub fn tokenize<L, I, P>(input: I, lexer: P) -> std::result::Result<TokenList<L>, Box<ParserError>>
where
    L: Lexeme,
    I: AsRef<str>,
    P: Parser<char, Vec<TokenSpan<L>>, Error = ParserError>,
{
    let mut token_list = TokenList::<L>::default();

    let len = input.as_ref().len();
    let iter = input
        .as_ref()
        .chars()
        .enumerate()
        .map(|(i, c)| (c, Span::new(i..i + 1)));
    let stream = Stream::from_iter(Span::new(len..len + 1), iter);

    let (tokens, mut errs) = lexer.parse_recovery(stream);

    if !errs.is_empty() {
        Err(errs.swap_remove(0).into())
    } else {
        if let Some(tokens) = tokens {
            // Note: Chumsky does not support stateful combinators at the moment.
            // Therefore we need a second pass over the vector of tokens to
            // internalize the strings and build the index list.
            tokens.into_iter().for_each(|(token, span)| {
                let new_token = token.internalize(&mut token_list);
                token_list.push((new_token, span));
            });
        }
        Ok(token_list)
    }
}
