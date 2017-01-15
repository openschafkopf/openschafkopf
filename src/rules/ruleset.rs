extern crate toml;

use primitives::*;
use rules::*;
use rules::rulesrufspiel::*;
use rules::rulessolo::*;
use rules::rulesramsch::*;
use rules::trumpfdecider::*;
use rules::payoutdecider::*;
use util::as_num::*;

use std::error::Error;
use std::fs::File;
use std::io::prelude::*;
use std::path::Path;

pub struct SRuleGroup {
    pub m_str_name : String,
    pub m_vecrules : Vec<Box<TActivelyPlayableRules>>,
}

pub enum VStockOrT<T> {
    Stock(/*n_stock*/isize), // number must be positive, but use isize since it is essentially a payment
    OrT(T),
}

pub struct SRuleSet {
    pub m_avecrulegroup : SPlayerIndexMap<Vec<SRuleGroup>>,
    pub m_stockorramsch : VStockOrT<Box<TRules>>,
}

pub fn allowed_rules(vecrulegroup: &[SRuleGroup]) -> Vec<&TActivelyPlayableRules> {
    vecrulegroup.iter().flat_map(|rulegroup| rulegroup.m_vecrules.iter().map(|rules| rules.as_ref())).collect()
}

impl SRuleSet {
    pub fn from_string(str_toml: &str) -> Result<SRuleSet, Vec<toml::ParserError>> {
        str_toml.parse::<toml::Value>().map(|tomltbl| {
            let read_int = |str_key: &str, n_default, n_ubound| {
                if let Some(n) = tomltbl.lookup(str_key).and_then(|tomlval| tomlval.as_integer()) {
                    if n_ubound<=n {
                        n
                    } else {
                        println!("Found {} with invalid value {}. Defaulting to {}.", str_key, n, n_default);
                        n_default
                    }
                } else {
                    println!("Could not find {}, defaulting to {}.", str_key, n_default);
                    n_default
                }
            };
            let read_payout = |str_payout, n_payout_default| read_int(str_payout, n_payout_default, 1).as_num::<isize>();
            let n_payout_rufspiel = read_payout("rufspiel-price", 20);
            let n_payout_schneider_schwarz_lauf = read_payout("extras-price", 10);
            let n_payout_single = read_payout("solo-price", 50);
            SRuleSet {
                m_avecrulegroup : create_playerindexmap(|eplayerindex| {
                    let mut vecrulegroup = Vec::new();
                    {
                        let mut create_rulegroup = |str_rule_name_file: &str, str_group_name: &str, vecrules| {
                            if let Some(tomlval_active_rules) = tomltbl.lookup("activerules") {
                                if tomlval_active_rules.lookup(str_rule_name_file).is_some() {
                                    vecrulegroup.push(SRuleGroup{
                                        m_str_name: str_group_name.to_string(),
                                        m_vecrules: vecrules
                                    });
                                }
                            }
                        };
                        let read_lauf = |str_game| read_int(&format!("activerules.{}.lauf", str_game), 3, 0).as_num::<usize>();
                        create_rulegroup(
                            "rufspiel",
                            "Rufspiel", 
                            EFarbe::values()
                                .filter(|efarbe| EFarbe::Herz!=*efarbe)
                                .map(|efarbe| Box::new(SRulesRufspiel{
                                    m_eplayerindex: eplayerindex,
                                    m_efarbe: efarbe,
                                    m_n_payout_base: n_payout_rufspiel,
                                    m_n_payout_schneider_schwarz: n_payout_schneider_schwarz_lauf,
                                    m_laufendeparams: SLaufendeParams::new(n_payout_schneider_schwarz_lauf, read_lauf("rufspiel")),
                                }) as Box<TActivelyPlayableRules>)
                                .collect()
                        );
                        // TODO improve interface to reduce boilerplate
                        macro_rules! read_sololike {
                            ($payoutdecider: ident, $fn_prio: expr, $str_rulename_suffix: expr) => {
                                let internal_rulename = |str_rulename| {
                                    format!("{}{}", str_rulename, $str_rulename_suffix)
                                };
                                macro_rules! generate_sololike_farbe {
                                    ($trumpfdecider: ident, $i_prioindex: expr, $rulename: expr, $n_payout_base: expr, $n_payout_schneider_schwarz: expr, $laufendeparams: expr) => {{
                                        macro_rules! internal_generate_sololike_farbe {
                                            ($farbedesignator: ident) => {
                                                sololike::<$trumpfdecider<STrumpfDeciderFarbe<$farbedesignator>>, $payoutdecider> (eplayerindex, $i_prioindex, &format!("{}-{}", $farbedesignator::farbe(), $rulename), $n_payout_base, $n_payout_schneider_schwarz, $laufendeparams)
                                            }
                                        }
                                        vec! [
                                            internal_generate_sololike_farbe!(SFarbeDesignatorEichel),
                                            internal_generate_sololike_farbe!(SFarbeDesignatorGras),
                                            internal_generate_sololike_farbe!(SFarbeDesignatorHerz),
                                            internal_generate_sololike_farbe!(SFarbeDesignatorSchelln),
                                        ]
                                    }}
                                }
                                let str_rulename = internal_rulename("Solo");
                                // TODO? make n_payout_base adjustable, n_payout_schneider_schwarz adjustable on a per-game basis?
                                create_rulegroup(
                                    "solo",
                                    &str_rulename,
                                    generate_sololike_farbe!(SCoreSolo, $fn_prio(0), &str_rulename, /*n_payout_base*/n_payout_single, /*n_payout_schneider_schwarz*/n_payout_schneider_schwarz_lauf, SLaufendeParams::new(n_payout_schneider_schwarz_lauf, read_lauf("solo")))
                                );
                                let str_rulename = internal_rulename("Wenz");
                                create_rulegroup(
                                    "wenz",
                                    &str_rulename,
                                    vec![sololike::<SCoreGenericWenz<STrumpfDeciderNoTrumpf>, $payoutdecider>(eplayerindex, $fn_prio(-1),&str_rulename, /*n_payout_base*/n_payout_single, /*n_payout_schneider_schwarz*/n_payout_schneider_schwarz_lauf, SLaufendeParams::new(n_payout_schneider_schwarz_lauf, read_lauf("wenz")))]
                                );
                                create_rulegroup(
                                    "farbwenz",
                                    &internal_rulename("Farbwenz"),
                                    generate_sololike_farbe!(SCoreGenericWenz, $fn_prio(-2), &internal_rulename("Wenz"), /*n_payout_base*/n_payout_single, /*n_payout_schneider_schwarz*/n_payout_schneider_schwarz_lauf, SLaufendeParams::new(n_payout_schneider_schwarz_lauf, read_lauf("farbwenz")))
                                );
                                let str_rulename = internal_rulename("Geier");
                                create_rulegroup(
                                    "geier",
                                    &str_rulename,
                                    vec![sololike::<SCoreGenericWenz<STrumpfDeciderNoTrumpf>, $payoutdecider>(eplayerindex, $fn_prio(-3),&str_rulename, /*n_payout_base*/n_payout_single, /*n_payout_schneider_schwarz*/n_payout_schneider_schwarz_lauf, SLaufendeParams::new(n_payout_schneider_schwarz_lauf, read_lauf("geier")))]
                                );
                                create_rulegroup(
                                    "farbgeier",
                                    &internal_rulename("Farbgeier"),
                                    generate_sololike_farbe!(SCoreGenericGeier, $fn_prio(-4), &internal_rulename("Geier"), /*n_payout_base*/n_payout_single, /*n_payout_schneider_schwarz*/n_payout_schneider_schwarz_lauf, SLaufendeParams::new(n_payout_schneider_schwarz_lauf, read_lauf("farbgeier")))
                                );
                            }
                        }
                        read_sololike!(SPayoutDeciderPointBased, |i_prioindex| VGameAnnouncementPriority::SoloLikeSimple(i_prioindex), "");
                        read_sololike!(SPayoutDeciderTout, |i_prioindex| VGameAnnouncementPriority::SoloTout(i_prioindex), " Tout");
                        create_rulegroup(
                            "solo",
                            "Sie",
                            vec![sololike::<SCoreSolo<STrumpfDeciderNoTrumpf>, SPayoutDeciderSie>(eplayerindex, VGameAnnouncementPriority::SoloSie ,&"Sie", /*n_payout_base*/n_payout_single, /*n_payout_schneider_schwarz*/n_payout_schneider_schwarz_lauf, SLaufendeParams::new(n_payout_schneider_schwarz_lauf, read_lauf("solo")))]
                        );
                    }
                    vecrulegroup
                }),
                m_stockorramsch : {
                    if tomltbl.lookup("noactive.ramsch").is_some() {
                        assert!(tomltbl.lookup("noactive.stock").is_none()); // TODO what to do in those cases? Better option to model alternatives? Allow stock *and* ramsch at the same time?
                        VStockOrT::OrT(Box::new(SRulesRamsch{
                            m_n_price: n_payout_schneider_schwarz_lauf,
                        }) as Box<TRules>) // TODO make adjustable
                    } else if tomltbl.lookup("noactive.stock").is_some() {
                        VStockOrT::Stock(n_payout_rufspiel) // TODO make adjustable
                    } else {
                        VStockOrT::Stock(0) // represent "no stock" by using a zero stock payment
                    }
                }
            }
        })
    }

    pub fn from_file(path: &Path) -> Result<SRuleSet, &'static str> {
        if !path.exists() {
            println!("File {} not found. Creating it.", path.display());
            let mut file = match File::create(&path) {
                Err(why) => panic!("Could not create {}: {}", path.display(), Error::description(&why)),
                Ok(file) => file,
            };
            // TODO: make creation of ruleset file adjustable
            for str_rules in &[
                "[activerules.rufspiel]",
                "[activerules.solo]",
                "[activerules.farbwenz]",
                "[activerules.wenz]",
                "[activerules.farbgeier]",
                "[activerules.geier]",
            ] {
                file.write_all(str_rules.as_bytes()).unwrap();
            }
        }
        assert!(path.exists()); 
        let mut file = match File::open(&path) {
            Err(why) => panic!("Could not open {}: {}", path.display(), Error::description(&why)),
            Ok(file) => file,
        };
        let mut str_toml = String::new();
        if let Ok(_n_bytes) = file.read_to_string(&mut str_toml) {
            Self::from_string(&str_toml).map_err(|_| "Parsing error") // TODO? error_chain
        } else {
            Err("IO error")
        }
    }
}

