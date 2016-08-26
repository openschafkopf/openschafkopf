extern crate rand;
extern crate ncurses;
#[macro_use]
extern crate itertools;
extern crate permutohedron;
extern crate clap;
extern crate arrayvec;

#[macro_use]
mod util;
mod primitives;
mod rules;
mod game;
mod player;
mod ai;
mod skui;

use game::*;
use std::sync::mpsc;
use primitives::*;
use rules::*;
use rules::rulesrufspiel::SRulesRufspiel;
use std::collections::HashSet;
use rules::ruleset::*;
use ai::*;
use std::path::Path;
use player::*;
use player::playerhuman::*;
use player::playercomputer::*;

fn main() {
    let clapmatches = clap::App::new("schafkopf")
        .arg(clap::Arg::with_name("rulesetpath")
             .long("ruleset")
             .default_value(".schafkopfruleset")
        )
        .arg(clap::Arg::with_name("numgames")
             .long("numgames")
             .default_value("4")
         )
        .arg(clap::Arg::with_name("ai")
             .long("ai")
             .default_value("cheating")
         )
        .get_matches();
    {
        let rules = SRulesRufspiel {
            m_eplayerindex : 0,
            m_efarbe : EFarbe::Gras,
        };
        let vecstich = {
            let mut vecstich_internal = Vec::new();
            {
                let mut add_stich = |eplayerindex, str_stich| {
                    vecstich_internal.push(SStich::new(eplayerindex));
                    for card in util::cardvectorparser::parse_cards(str_stich).iter().cycle().skip(eplayerindex).take(4) {
                        vecstich_internal.last_mut().unwrap().zugeben(card.clone());
                    }
                };
                add_stich(0, "g7 g8 ga g9");
                add_stich(2, "s8 ho s7 s9");
                add_stich(1, "h7 hk hu su");
                add_stich(2, "eo go hz h8");
                add_stich(2, "e9 ek e8 ea");
                add_stich(3, "sa eu so ha");
            }
            vecstich_internal
        };

        let mut mapeplayerindexsettrumpforfarbe = create_playerindexmap(|_eplayerindex| {
            let mut settrumpforfarbe = HashSet::new();
            settrumpforfarbe.insert(VTrumpfOrFarbe::Trumpf);
            for efarbe in EFarbe::all_values().iter() {
                settrumpforfarbe.insert(VTrumpfOrFarbe::Farbe(*efarbe));
            }
            settrumpforfarbe
        });
        for stich in vecstich.iter() {
            println!("{}", stich);
            let trumpforfarbe_first_card = rules.trumpf_or_farbe(stich.first_card());
            for (eplayerindex, card) in stich.indices_and_cards() {
                let trumpforfarbe = rules.trumpf_or_farbe(card);
                if trumpforfarbe_first_card != trumpforfarbe {
                    println!("Removing {} from player {}",
                        match trumpforfarbe_first_card {
                            VTrumpfOrFarbe::Trumpf => "Trumpf".to_string(),
                            VTrumpfOrFarbe::Farbe(efarbe) => format!("{}", efarbe),
                        },
                        eplayerindex
                    );
                    mapeplayerindexsettrumpforfarbe[eplayerindex].remove(&trumpforfarbe_first_card);
                } else {
                    // assert consistency over the whole game
                    assert!(mapeplayerindexsettrumpforfarbe[eplayerindex].contains(&trumpforfarbe_first_card));
                }
            }
        }

        let mut n_susp = 0;
        ai::handiterators::for_each_possible_hand(
            &vecstich,
            &SHand::new_from_vec(util::cardvectorparser::parse_cards("gk sk").into_iter().collect()),
            0, // eplayerindex
            |ahand| {
                ahand.iter()
                    .enumerate()
                    .all(|(eplayerindex, hand)| {
                        hand.cards().iter()
                            .map(|card| rules.trumpf_or_farbe(card.clone()))
                            .all(|trumpforfarbe| {
                                mapeplayerindexsettrumpforfarbe[eplayerindex].contains(&trumpforfarbe)
                            })
                    })
            },
            |ahand| {
                let mut susp = suspicion::SSuspicion::new_from_raw(/*eplayerindex*/0, ahand);
                n_susp += 1;
                println!("{} {} {} {}",
                    susp.hands()[0],
                    susp.hands()[1],
                    susp.hands()[2],
                    susp.hands()[3]
                );
                susp.compute_successors(&rules, &mut vecstich.clone(), &|_vecstich_complete, vecstich_successor| {
                    assert!(!vecstich_successor.is_empty());
                    random_sample_from_vec(vecstich_successor, 1);
                });
            }
        );
        println!("{} suspicions", n_susp);
    }

    let ai : Box<TAi> = {
        match clapmatches.value_of("ai").unwrap().as_ref() {
            "cheating" => Box::new(ai::SAiCheating{}),
            "simulating" => Box::new(ai::SAiSimulating{}),
            _ => {
                println!("Warning: AI not recognized. Defaulting to 'cheating'");
                Box::new(ai::SAiCheating{})
            }
        }
    };

    let ruleset = read_ruleset(Path::new(clapmatches.value_of("rulesetpath").unwrap()));
    {
        let hand_fixed = random_hand(8, &mut SCard::all_values().into_iter().map(|card| Some(card)).collect());
        let eplayerindex_fixed = 0;

        println!("Hand: {}", hand_fixed);
        for rules in allowed_rules(&ruleset.m_avecrulegroup[eplayerindex_fixed]).iter() 
            .filter(|rules| rules.can_be_played(&hand_fixed))
        {
            let f_payout_avg = ai.rank_rules(&hand_fixed, eplayerindex_fixed, rules.clone(), 100);
            println!("{}", rules);
            println!("{}", f_payout_avg);
        }
    }
    //return;

    skui::init_ui();
    let mut vecplayer : Vec<Box<TPlayer>> = vec![
        Box::new(SPlayerHuman{m_ai : ai.as_ref()}),
        Box::new(SPlayerComputer{m_ai : ai.as_ref()}),
        Box::new(SPlayerComputer{m_ai : ai.as_ref()}),
        Box::new(SPlayerComputer{m_ai : ai.as_ref()})
    ];
    let mut accountbalance = SAccountBalance::new();
    for i_game in 0..clapmatches.value_of("numgames").unwrap().parse::<usize>().unwrap_or(4) {
        let mut gamepreparations = SGamePreparations::new(
            &ruleset,
            /*eplayerindex_first*/i_game % 4,
        );
        while let Some(eplayerindex) = gamepreparations.which_player_can_do_something() {
            skui::logln(&format!("Asking player {} for game", eplayerindex));
            let orules = vecplayer[eplayerindex].ask_for_game(
                &gamepreparations.m_ahand[eplayerindex],
                &gamepreparations.m_vecgameannouncement,
                &gamepreparations.m_ruleset.m_avecrulegroup[eplayerindex]
            );
            gamepreparations.announce_game(eplayerindex, orules).unwrap();
        }
        skui::logln("Asked players if they want to play. Determining rules");
        if let Some(mut game) = gamepreparations.determine_rules() {
            while let Some(eplayerindex)=game.which_player_can_do_something() {
                let (txcard, rxcard) = mpsc::channel::<SCard>();
                vecplayer[eplayerindex].take_control(
                    &game,
                    txcard.clone()
                );
                let card_played = rxcard.recv().unwrap();
                game.zugeben(card_played, eplayerindex).unwrap();
            }
            skui::logln("Results");
            for eplayerindex in 0..4 {
                skui::logln(&format!("Player {}: {} points", eplayerindex, game.points_per_player(eplayerindex)));
            }
            accountbalance.apply_payout(&game.payout());
        }
        skui::print_account_balance(&accountbalance);
    }
    skui::end_ui();
    println!("Results: {}", skui::account_balance_string(&accountbalance));
}
