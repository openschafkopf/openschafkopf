extern crate combine;

use primitives::card::*;
use self::combine::*;
use self::combine::primitives::Stream;

// For now, parsing is only used to simplify input in programming.
// But it is clear that these methods are far from perfect.
// TODO: case-insensitive parsing
// TODO: error handling
// TODO farbe_parse and schlag_parse should be simplified (eliminate "duplicate enumeration")
// TODO: enable parsing stuff like egho gazk9 s7

fn farbe_parse<I>(input: State<I>) -> ParseResult<EFarbe, I>
where I: Stream<Item=char> {
    combine::choice([
        combine::char('e'),
        combine::char('g'),
        combine::char('h'),
        combine::char('s'),
    ])
    .map(|chr_farbe| {
        match chr_farbe {
            'e' => EFarbe::Eichel,
            'g' => EFarbe::Gras,
            'h' => EFarbe::Herz,
            's' => EFarbe::Schelln,
            _ => unreachable!(),
        }
    } )
    .parse_state(input)
}

fn schlag_parse<I>(input: State<I>) -> ParseResult<ESchlag, I>
where I: Stream<Item=char> {
    combine::choice([
        combine::char('7'),
        combine::char('8'),
        combine::char('9'),
        combine::char('z'),
        combine::char('u'),
        combine::char('o'),
        combine::char('k'),
        combine::char('a'),
    ])
    .map(|chr_farbe| {
        match chr_farbe {
            '7' => ESchlag::S7,
            '8' => ESchlag::S8,
            '9' => ESchlag::S9,
            'z' => ESchlag::Zehn,
            'u' => ESchlag::Unter,
            'o' => ESchlag::Ober,
            'k' => ESchlag::Koenig,
            'a' => ESchlag::Ass,
            _ => unreachable!(),
        }
    } )
    .parse_state(input)
}

fn card_parse<I>(input: State<I>) -> ParseResult<SCard, I>
where I: Stream<Item=char> {
    (parser(farbe_parse), parser(schlag_parse))
        .map(|(efarbe, eschlag)| SCard::new(efarbe, eschlag))
        .parse_state(input)
}

pub fn parse_cards(str_cards: &str) -> Vec<SCard> {
    sep_by::<Vec<_>,_,_>(parser(card_parse), spaces())
        .parse(str_cards)
        .unwrap()
        .0
}

#[test]
fn test_cardvectorparser() {
    let veccard = parse_cards("ek gk hz hu s7");
    assert_eq!(veccard.len(), 5);
    assert!(veccard[1] == SCard::new(EFarbe::Gras, ESchlag::Koenig));
}