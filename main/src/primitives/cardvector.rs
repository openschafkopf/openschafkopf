use crate::primitives::card::*;
use crate::combine::{*, char::*};

// adapted from https://docs.rs/combine/3.6.7/combine/index.html#examples
pub fn card_parser<I: Stream<Item=char>>() -> impl Parser<Input = I, Output = SCard>
    where I::Error: ParseError<I::Item, I::Range, I::Position>, // Necessary due to rust-lang/rust#24159
{
    (
        choice!(
            choice!(char('e'), char('E')).map(|_chr| EFarbe::Eichel),
            choice!(char('g'), char('G')).map(|_chr| EFarbe::Gras),
            choice!(char('h'), char('H')).map(|_chr| EFarbe::Herz),
            choice!(char('s'), char('S')).map(|_chr| EFarbe::Schelln)
        ),
        choice!(
            char('7').map(|_chr| ESchlag::S7),
            char('8').map(|_chr| ESchlag::S8),
            char('9').map(|_chr| ESchlag::S9),
            choice!(char('z'), char('Z'), char('x'), char('X')).map(|_chr| ESchlag::Zehn),
            choice!(char('u'), char('U')).map(|_chr| ESchlag::Unter),
            choice!(char('o'), char('O')).map(|_chr| ESchlag::Ober),
            choice!(char('k'), char('K')).map(|_chr| ESchlag::Koenig),
            choice!(char('a'), char('A')).map(|_chr| ESchlag::Ass)
        )
    ).map(|(efarbe, eschlag)| SCard::new(efarbe, eschlag))
}

pub fn parse_cards<C: std::iter::Extend<SCard>+Default>(str_cards: &str) -> Option<C> {
    spaces()
        .with(sep_by::<C,_,_>(card_parser(), spaces()))
        .skip(spaces())
        .skip(eof())
        // end of parser
        .parse(str_cards)
        .ok()
        .map(|pairoutconsumed| pairoutconsumed.0)
}

#[test]
fn test_cardvectorparser() {
    use crate::util::*;
    assert_eq!(
        verify!(parse_cards::<Vec<_>>("ek Gk hZ hu s7 gZ")).unwrap(),
        vec![
            SCard::new(EFarbe::Eichel, ESchlag::Koenig),
            SCard::new(EFarbe::Gras, ESchlag::Koenig),
            SCard::new(EFarbe::Herz, ESchlag::Zehn),
            SCard::new(EFarbe::Herz, ESchlag::Unter),
            SCard::new(EFarbe::Schelln, ESchlag::S7),
            SCard::new(EFarbe::Gras, ESchlag::Zehn),
        ]
    );
}