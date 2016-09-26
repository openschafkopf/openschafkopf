use primitives::*;

// thin wrappers ensuring invariants

pub struct SFullHand<'hand> {
    m_hand: &'hand SHand,
}

impl<'hand> SFullHand<'hand> {
    pub fn new(hand: &SHand) -> SFullHand {
        assert_eq!(hand.cards().len(), 8);
        SFullHand {
            m_hand : hand,
        }
    }
    pub fn get(&self) -> &SHand {
        self.m_hand
    }
}

pub struct SGameFinishedStiche<'vecstich> {
    m_vecstich: &'vecstich Vec<SStich>,
}

impl<'vecstich> SGameFinishedStiche<'vecstich> {
    pub fn new(vecstich: &Vec<SStich>) -> SGameFinishedStiche {
        assert_eq!(vecstich.len(), 8);
        assert!(vecstich.iter().all(|stich| 4==stich.size()));
        SGameFinishedStiche {
            m_vecstich : vecstich,
        }
    }
    pub fn get(&self) -> &Vec<SStich> {
        self.m_vecstich
    }
}
