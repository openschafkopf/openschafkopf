use card::*;
use hand::*;
use stich::*;
use rules::*;
use rules::ruleset::*;
use player::*;
use skui;

use rand::{self, Rng};

pub struct SGamePreparations<'rules> {
    pub m_ahand : [SHand; 4],
    m_ruleset : &'rules SRuleSet,
}

pub fn random_hand(n_size: usize, vecocard : &mut Vec<Option<SCard>>) -> SHand {
    let n_card_total = 32;
    assert_eq!(vecocard.len(), n_card_total);
    assert!(vecocard.iter().filter(|ocard| ocard.is_some()).count()>=n_size);
    SHand::new_from_vec({
        let mut veccard = SHandVector::new();
        for _i in 0..n_size {
            let mut i_card = rand::thread_rng().gen_range(0, n_card_total);
            while vecocard[i_card].is_none() {
                i_card = rand::thread_rng().gen_range(0, n_card_total);
            }
            veccard.push(vecocard[i_card].unwrap());
            vecocard[i_card] = None;
        }
        assert_eq!(veccard.len(), n_size);
        veccard
    })
}

pub fn random_hands() -> [SHand; 4] {
    let mut vecocard : Vec<Option<SCard>> = SCard::all_values().into_iter().map(|card| Some(card)).collect();
    assert!(vecocard.len()==32);
    create_playerindexmap(move |_eplayerindex|
        random_hand(8, &mut vecocard)
    )
}

impl<'rules> SGamePreparations<'rules> {
    pub fn new(ruleset : &'rules SRuleSet) -> SGamePreparations<'rules> {
        SGamePreparations {
            m_ahand : random_hands(),
            m_ruleset : ruleset,
        }
    }

    // TODO: extend return value to support stock, etc.
    // TODO: eliminate vecplayer and substitute start_game by which_player_can_do_something (similar to SGame)
    pub fn start_game<'players>(mut self, eplayerindex_first : EPlayerIndex, vecplayer: &Vec<Box<TPlayer+'players>>) -> Option<SGame<'rules>> {
        // prepare
        skui::logln("Preparing game");
        for hand in self.m_ahand.iter() {
            skui::log(&format!("{} |", hand));
        }
        skui::logln("");

        // decide which game is played
        skui::logln("Asking players if they want to play");
        let mut vecgameannouncement : Vec<SGameAnnouncement> = Vec::new();
        for eplayerindex in (eplayerindex_first..eplayerindex_first+4).map(|eplayerindex| eplayerindex%4) {
            let orules = vecplayer[eplayerindex].ask_for_game(
                &self.m_ahand[eplayerindex],
                &vecgameannouncement,
                &self.m_ruleset.m_avecrulegroup[eplayerindex]
            );
            assert!(orules.as_ref().map_or(true, |rules| eplayerindex==rules.playerindex().unwrap()));
            vecgameannouncement.push(SGameAnnouncement{
                m_eplayerindex : eplayerindex, 
                m_opairrulespriority : orules.map(|rules| (
                    rules,
                    0 // priority, TODO determine priority
                )),
            });
        }
        skui::logln("Asked players if they want to play. Determining rules");
        // TODO: find sensible way to deal with multiple game announcements (currently, we choose highest priority)
        assert!(!vecgameannouncement.is_empty());
        vecgameannouncement.iter()
            .map(|gameannouncement| gameannouncement.m_opairrulespriority)
            .max_by_key(|opairrulespriority| opairrulespriority.map(|(_orules, priority)| priority)) 
            .unwrap()
            .map(move |(rules, _priority)| {
                assert!(rules.playerindex().is_some());
                skui::logln(&format!(
                    "Rules determined ({} plays {}). Sorting hands",
                    rules.playerindex().unwrap(),
                    rules
                ));
                for hand in self.m_ahand.iter_mut() {
                    rules.sort_cards_first_trumpf_then_farbe(hand.cards_mut());
                    skui::logln(&format!("{}", hand));
                }
                SGame {
                    m_ahand : self.m_ahand,
                    m_rules : rules,
                    m_vecstich : vec![SStich::new(eplayerindex_first)],
                }
            })
    }
}

pub struct SGame<'rules> {
    pub m_ahand : [SHand; 4],
    pub m_rules : &'rules TRules,
    pub m_vecstich : Vec<SStich>,
}

pub type SGameAnnouncementPriority = isize;

pub struct SGameAnnouncement<'rules> {
    pub m_eplayerindex : EPlayerIndex,
    pub m_opairrulespriority : Option<(&'rules TRules, SGameAnnouncementPriority)>,
}

impl<'rules> SGame<'rules> {
    pub fn which_player_can_do_something(&self) -> Option<EPlayerIndex> {
        if 8==self.m_vecstich.len() && 4==self.m_vecstich.last().unwrap().size() {
            None
        } else {
            Some(
                (self.m_vecstich.last().unwrap().first_player_index() + self.m_vecstich.last().unwrap().size() ) % 4
            )
        }
    }

    pub fn zugeben(&mut self, card_played: SCard, eplayerindex: EPlayerIndex) -> EPlayerIndex { // TODO: should invalid inputs be indicated by return value?
        // returns the EPlayerIndex of the player who is the next in row to do something
        // TODO: how to cope with finished game?
        skui::logln(&format!("Player {} wants to play {}", eplayerindex, card_played));
        assert_eq!(eplayerindex, self.which_player_can_do_something().unwrap());
        assert!(self.m_ahand[eplayerindex].contains(card_played));
        {
            let ref mut hand = self.m_ahand[eplayerindex];
            assert!(self.m_rules.card_is_allowed(&self.m_vecstich, hand, card_played));
            hand.play_card(card_played);
            self.m_vecstich.last_mut().unwrap().zugeben(card_played);
        }
        for eplayerindex in 0..4 {
            skui::logln(&format!("Hand {}: {}", eplayerindex, self.m_ahand[eplayerindex]));
        }
        if 4==self.m_vecstich.last().unwrap().size() {
            if 8==self.m_vecstich.len() { // TODO kurze Karte?
                skui::logln("Game finished.");
                skui::print_vecstich(&self.m_vecstich);
                self.notify_game_listeners();
                (self.m_vecstich.first().unwrap().first_player_index() + 1) % 4 // for next game
            } else {
                // TODO: all players should have to acknowledge the current stich in some way
                let eplayerindex_last_stich = {
                    let stich = self.m_vecstich.last().unwrap();
                    skui::logln(&format!("Stich: {}", stich));
                    let eplayerindex_last_stich = self.m_rules.winner_index(stich);
                    skui::logln(&format!("{} made by {}, ({} points)",
                        stich,
                        eplayerindex_last_stich,
                        self.m_rules.points_stich(stich)
                    ));
                    eplayerindex_last_stich
                };
                skui::logln(&format!("Opening new stich starting at {}", eplayerindex_last_stich));
                assert!(self.m_vecstich.is_empty() || 4==self.m_vecstich.last().unwrap().size());
                self.m_vecstich.push(SStich::new(eplayerindex_last_stich));
                self.notify_game_listeners();

                self.notify_game_listeners();
                eplayerindex_last_stich
            }
        } else {
            self.notify_game_listeners();
            (eplayerindex + 1) % 4
        }
    }

    pub fn points_per_player(&self, eplayerindex: EPlayerIndex) -> isize {
        self.m_rules.points_per_player(&self.m_vecstich, eplayerindex)
    }

    fn notify_game_listeners(&self) {
        // TODO: notify game listeners
    }

    pub fn payout(&self) -> [isize; 4] {
        self.m_rules.payout(&self.m_vecstich)
    }
}
