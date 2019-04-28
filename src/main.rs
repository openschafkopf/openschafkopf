#![cfg_attr(feature="cargo-clippy", allow(clippy::block_in_if_condition_stmt))]
#![deny(bare_trait_objects)]

extern crate rand;
extern crate ncurses;
#[macro_use]
extern crate itertools;
extern crate permutohedron;
#[macro_use]
extern crate clap;
extern crate arrayvec;
extern crate rayon;
#[macro_use]
extern crate failure;
extern crate as_num;
extern crate plain_enum;
#[macro_use]
extern crate derive_new;
extern crate toml;
#[macro_use]
extern crate log;
extern crate env_logger;
extern crate chrono;

#[macro_use]
mod util;
mod primitives;
mod rules;
mod game;
mod player;
mod ai;
mod skui;
mod subcommands;

use crate::primitives::*;
use crate::rules:: ruleset::*;
use crate::ai::*;
use std::path::Path;
use crate::player::{
    *,
    playerhuman::*,
    playercomputer::*,
};
use crate::util::*;

fn main() -> Result<(), Error> {
    env_logger::init();
    let clap_arg = |str_long, str_default| {
        clap::Arg::with_name(str_long)
            .long(str_long)
            .default_value(str_default)
    };
    // TODO clean up command line arguments and possibly avoid repetitions
    let clapmatches = clap::App::new("schafkopf")
        .subcommand(clap::SubCommand::with_name("cli")
            .arg(clap_arg("ruleset", "rulesets/default.toml"))
            .arg(clap_arg("ai", "cheating"))
            .arg(clap_arg("numgames", "4"))
        )
        .subcommand(clap::SubCommand::with_name("rank-rules")
            .arg(clap_arg("ruleset", "rulesets/default.toml"))
            .arg(clap_arg("ai", "cheating"))
            .arg(clap_arg("hand", ""))
            .arg(clap_arg("position", "0"))
        )
        .get_matches();
    let ai = |subcommand_matches: &clap::ArgMatches| {
        match debug_verify!(subcommand_matches.value_of("ai")).unwrap() {
            "cheating" => SAi::new_cheating(/*n_rank_rules_samples*/50, /*n_suggest_card_branches*/2),
            "simulating" => 
                SAi::new_simulating(
                    /*n_rank_rules_samples*/50,
                    /*n_suggest_card_branches*/2,
                    /*n_suggest_card_samples*/10,
                ),
            _ => {
                println!("Warning: AI not recognized. Defaulting to 'cheating'");
                SAi::new_cheating(/*n_rank_rules_samples*/50, /*n_suggest_card_branches*/2)
            }
        }
    };
    fn get_ruleset(subcommand_matches: &clap::ArgMatches) -> Result<SRuleSet, Error> {
        SRuleSet::from_file(Path::new(debug_verify!(subcommand_matches.value_of("ruleset")).unwrap()))
    }
    if let Some(subcommand_matches)=clapmatches.subcommand_matches("rank-rules") {
        let ruleset = get_ruleset(subcommand_matches)?;
        let str_hand = subcommand_matches.value_of("hand").ok_or_else(||format_err!("No hand given as parameter."))?;
        let hand = SHand::new_from_vec(cardvector::parse_cards(str_hand).ok_or_else(||format_err!("Could not parse hand."))?);
        let hand = Some(hand).filter(|hand| hand.cards().len()==ruleset.ekurzlang.cards_per_player()).ok_or_else(||format_err!("Could not convert hand to a full hand of cards"))?;
        return Ok(subcommands::rank_rules::rank_rules(
            &ruleset,
            SFullHand::new(&hand, ruleset.ekurzlang),
            /*epi_rank*/value_t!(subcommand_matches.value_of("position"), EPlayerIndex).unwrap_or(EPlayerIndex::EPI0),
            &ai(subcommand_matches),
        ));
    }
    if let Some(subcommand_matches)=clapmatches.subcommand_matches("cli") {
        return Ok(subcommands::cli::game_loop_cli(
            &EPlayerIndex::map_from_fn(|epi| -> Box<dyn TPlayer> {
                if EPlayerIndex::EPI1==epi {
                    Box::new(SPlayerHuman{ai : ai(subcommand_matches)})
                } else {
                    Box::new(SPlayerComputer{ai: ai(subcommand_matches)})
                }
            }),
            /*n_games*/ debug_verify!(subcommand_matches.value_of("numgames")).unwrap().parse::<usize>().unwrap_or(4),
            &get_ruleset(subcommand_matches)?,
        ));
    }
    Ok(())
}


