use card::*;
use std::fmt;
use std::iter;

use std::ops::Add;
use std::ops::Index;
use std::ops::IndexMut;

pub type EPlayerIndex = usize; // TODO: would a real enum be more adequate?

pub struct CStich {
    pub m_eplayerindex_first: EPlayerIndex,
    m_n_size: usize,
    pub m_acard: [CCard; 4],
}

struct StichIterator<'stich> {
    m_eplayerindex : EPlayerIndex,
    m_stich: &'stich CStich,
}

impl<'stich> Iterator for StichIterator<'stich> {
    type Item = (EPlayerIndex, CCard);
    fn next(&mut self) -> Option<(EPlayerIndex, CCard)> {
        if self.m_eplayerindex==self.m_stich.size() {
            return None;
        }
        else {
            let i_index = self.m_stich.m_eplayerindex_first + self.m_eplayerindex;
            let pairicard = (i_index, self.m_stich[i_index]);
            self.m_eplayerindex = self.m_eplayerindex + 1;
            return Some(pairicard);
        }
    }
}

impl Index<EPlayerIndex> for CStich {
    type Output = CCard;
    fn index<'a>(&'a self, eplayerindex : EPlayerIndex) -> &'a CCard {
        &self.m_acard[eplayerindex]
    }
}

impl IndexMut<EPlayerIndex> for CStich {
    fn index_mut<'a>(&'a mut self, eplayerindex: EPlayerIndex) -> &'a mut CCard {
        &mut self.m_acard[eplayerindex]
    }
}

impl CStich {
    pub fn new(eplayerindex_first: EPlayerIndex) -> CStich {
        CStich {
            m_eplayerindex_first : eplayerindex_first,
            m_n_size: 0,
            m_acard: [CCard::new(EFarbe::efarbeEICHEL, ESchlag::eschlag7); 4]
        }
    }
    pub fn empty(&self) -> bool {
        self.m_n_size == 0
    }
    pub fn first_player_index(&self) -> EPlayerIndex {
        self.m_eplayerindex_first
    }
    pub fn current_player_index(&self) -> EPlayerIndex {
        self.first_player_index() + self.size()
    }
    pub fn size(&self) -> usize {
        self.m_n_size
    }
    pub fn zugeben(&mut self, card: CCard) {
        assert!(self.m_n_size<4);
        let eplayerindex = (self.m_eplayerindex_first + self.m_n_size)%4;
        self[eplayerindex] = card; // sad: can not inline eplayerindex (borrowing)
        self.m_n_size = self.m_n_size + 1;
    }
    pub fn undo_most_recent_card(&mut self) {
        assert!(0 < self.m_n_size);
        self.m_n_size = self.m_n_size - 1;
    }
    pub fn set_card_by_offset(&mut self, i: usize, card: CCard) {
        let eplayerindex = self.m_eplayerindex_first + i;
        self[eplayerindex] = card; // sad: can not inline eplayerindex (borrowing)
    }
    pub fn first_card(&self) -> CCard {
        self[self.m_eplayerindex_first]
    }
    pub fn indices_and_cards(&self) -> StichIterator {
        StichIterator {
            m_eplayerindex: 0,
            m_stich: self
        }
    }
}

impl fmt::Display for CStich {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // TODO: more elegance!
        let mut vecocard = iter::repeat(None).take(4).collect::<Vec<Option<CCard>>>();
        for (i_player, card) in self.indices_and_cards() {
            vecocard[i_player as usize] = Some(card);
        }
        for (i, ocard) in vecocard.into_iter().enumerate() {
            if i==self.m_eplayerindex_first {
                write!(f, ">");
            } else {
                write!(f, " ");
            }
            match ocard {
                None => {write!(f, "__");}
                Some(card) => {write!(f, "{}", card);}
            }
        }
        write!(f, "")
    }
}

#[test]
fn test_stich() {
    // TODO: use quicktest or similar and check proper retrieval
    for eplayerindex in 0..4 {
        let mut stich = CStich::new(eplayerindex);
        for i_size in 0..4 {
            stich.zugeben(CCard::new(EFarbe::efarbeEICHEL, ESchlag::eschlag7));
            assert_eq!(stich.size(), i_size+1);
        }
        assert_eq!(stich.first_player_index(), eplayerindex);
    }
    // TODO: indices_and_cards
}
