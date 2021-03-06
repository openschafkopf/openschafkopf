use crate::game::*;
use crate::primitives::*;

// thin wrappers ensuring invariants

#[derive(Copy, Clone)]
pub struct SFullHand<'hand>(&'hand SHand);

impl<'hand> SFullHand<'hand> {
    pub fn new(hand: &SHand, ekurzlang: EKurzLang) -> SFullHand {
        assert_eq!(hand.cards().len(), ekurzlang.cards_per_player());
        SFullHand(hand)
    }
    pub fn get(self) -> &'hand SHand {
        self.0
    }
}

#[derive(Copy, Clone)]
pub struct SStichSequenceGameFinished<'stichseq>(&'stichseq SStichSequence);

impl SStichSequenceGameFinished<'_> {
    pub fn new(stichseq: &SStichSequence) -> SStichSequenceGameFinished {
        assert!(stichseq.game_finished());
        SStichSequenceGameFinished(stichseq)
    }
    pub fn get(&self) -> &SStichSequence {
        self.0
    }
}
