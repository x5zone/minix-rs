//! Board dealing: squares, money, and dice.
//!
//! Ground truth: `minix3/games/monop/` (board play in `monop.c`, card decks
//! in `cards.c` with `deck.h`, houses in `houses.c`, jail in `jail.c`). Money
//! only moves down to zero (below zero means bankruptcy); dice wrap around
//! the board. Card text and house rules stay with data and later work; this
//! module owns money arithmetic and movement.

use crate::TextGameError;

/// Starting money in dollars.
pub const START_MONEY: i32 = 1500;

/// Squares around the board.
pub const BOARD_SQUARES: u32 = 40;

/// Move `position` forward by `steps` squares with board wrap.
pub fn advance_position(position: u32, steps: u32) -> Result<u32, TextGameError> {
    if position >= BOARD_SQUARES {
        return Err(TextGameError::InvalidArgument);
    }
    Ok((position + steps) % BOARD_SQUARES)
}

/// True when the move passes (or lands on) the start square.
pub fn passes_start(position: u32, steps: u32) -> Result<bool, TextGameError> {
    if position >= BOARD_SQUARES {
        return Err(TextGameError::InvalidArgument);
    }
    Ok(position + steps >= BOARD_SQUARES)
}

/// Pay `amount` from `balance`: returns the new balance, or bankruptcy when
/// the balance would drop below zero.
pub fn pay(balance: i32, amount: u32) -> Result<i32, TextGameError> {
    if balance < 0 || amount > i32::MAX as u32 {
        return Err(TextGameError::OutOfRange);
    }
    let rest = balance - amount.min(i32::MAX as u32) as i32;
    if rest < 0 {
        return Err(TextGameError::OutOfRange);
    }
    Ok(rest)
}

/// Receive `amount` into `balance` (saturates instead of overflowing).
pub fn receive(balance: i32, amount: u32) -> i32 {
    balance.saturating_add(amount.min(i32::MAX as u32) as i32)
}

/// Roll two dice from raw values (each one through six).
pub fn dice_total(first: u32, second: u32) -> Result<u32, TextGameError> {
    if !(1..=6).contains(&first) || !(1..=6).contains(&second) {
        return Err(TextGameError::InvalidArgument);
    }
    Ok(first + second)
}

/// True on doubles (which grant another turn, and three in a row grant jail).
pub fn is_doubles(first: u32, second: u32) -> Result<bool, TextGameError> {
    if !(1..=6).contains(&first) || !(1..=6).contains(&second) {
        return Err(TextGameError::InvalidArgument);
    }
    Ok(first == second)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_advance_wraps() {
        assert_eq!(advance_position(39, 2).unwrap(), 1);
        assert_eq!(advance_position(0, 5).unwrap(), 5);
        assert_eq!(
            advance_position(40, 1).map(|_| ()),
            Err(TextGameError::InvalidArgument)
        );
    }

    #[test]
    fn test_start_passing() {
        assert!(passes_start(39, 2).unwrap());
        assert!(!passes_start(0, 5).unwrap());
    }

    #[test]
    fn test_pay_and_bankruptcy() {
        assert_eq!(pay(1500, 200).unwrap(), 1300);
        assert_eq!(pay(100, 200), Err(TextGameError::OutOfRange));
    }

    #[test]
    fn test_receive_saturates() {
        assert_eq!(receive(1500, 200), 1700);
        assert_eq!(receive(i32::MAX, 1), i32::MAX);
    }

    #[test]
    fn test_dice_checked() {
        assert_eq!(dice_total(3, 4).unwrap(), 7);
        assert_eq!(dice_total(0, 4).map(|_| ()), Err(TextGameError::InvalidArgument));
        assert!(is_doubles(4, 4).unwrap());
        assert!(!is_doubles(4, 5).unwrap());
    }
}
