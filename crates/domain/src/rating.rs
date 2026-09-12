#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rating {
    Again = 1,
    Hard = 2,
    Good = 3,
    Easy = 4,
}

impl Rating {
    /// FSRS four standard grades: 1=Again, 2=Hard, 3=Good, 4=Easy.
    pub fn as_grade(self) -> i64 {
        self as i64
    }

    pub fn from_grade(grade: i64) -> Option<Self> {
        match grade {
            1 => Some(Self::Again),
            2 => Some(Self::Hard),
            3 => Some(Self::Good),
            4 => Some(Self::Easy),
            _ => None,
        }
    }
}
