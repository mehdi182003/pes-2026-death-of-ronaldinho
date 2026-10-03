//! Goals that count: the score, the celebration, the kick-off.

use bevy::prelude::*;
use bevy_rapier3d::prelude::Velocity;

use crate::ball::{self, Ball};
use crate::pitch;
use crate::player::{self, Controlled};

/// Seconds between a goal and the kick-off.
const CELEBRATION: f32 = 3.0;

/// Where the ball and the player go for a kick-off.
pub const BALL_KICK_OFF: Vec3 = Vec3::new(0.0, ball::RADIUS, 0.0);
pub const PLAYER_KICK_OFF: Vec3 = Vec3::new(-1.2, 0.0, 0.3);

/// The two teams: the player's attacks the goal at +X.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Team {
    Home,
    Away,
}

#[derive(Resource, Default)]
pub struct Score {
    pub home: u32,
    pub away: u32,
    /// Seconds left before the kick-off, after a goal.
    celebrating: Option<f32>,
}

/// The score on screen, and the goal message.
#[derive(Component)]
pub struct ScoreBoard;
#[derive(Component)]
pub struct GoalBanner;

pub struct GoalsPlugin;

impl Plugin for GoalsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Score>()
            .add_systems(Update, (count_goals, kick_off, show_score).chain());
    }
}

/// The team that scores when the ball is at `position`: the whole ball past
/// a goal line, between the posts and under the crossbar.
pub fn goal_for(position: Vec3) -> Option<Team> {
    let inside_frame =
        position.z.abs() < pitch::GOAL_WIDTH / 2.0 && position.y < pitch::GOAL_HEIGHT;
    let past = |side: f32| (position.x - pitch::goal_line(side)) * side > ball::RADIUS;
    match () {
        _ if !inside_frame => None,
        _ if past(1.0) => Some(Team::Home),
        _ if past(-1.0) => Some(Team::Away),
        _ => None,
    }
}

fn count_goals(mut score: ResMut<Score>, balls: Query<&Transform, With<Ball>>) {
    if score.celebrating.is_some() {
        return;
    }
    let Ok(ball) = balls.single() else {
        return;
    };
    match goal_for(ball.translation) {
        Some(Team::Home) => score.home += 1,
        Some(Team::Away) => score.away += 1,
        None => return,
    }
    score.celebrating = Some(CELEBRATION);
}

/// After the celebration, the ball and the player go back to the centre.
fn kick_off(
    time: Res<Time>,
    mut score: ResMut<Score>,
    mut balls: Query<(&mut Transform, &mut Velocity), With<Ball>>,
    mut players: Query<(&mut Controlled, &mut Transform), Without<Ball>>,
) {
    let Some(left) = score.celebrating.as_mut() else {
        return;
    };
    *left -= time.delta_secs();
    if *left > 0.0 {
        return;
    }
    score.celebrating = None;
    for (mut transform, mut velocity) in &mut balls {
        transform.translation = BALL_KICK_OFF;
        *velocity = Velocity::zero();
    }
    for (mut controlled, mut transform) in &mut players {
        player::reset(&mut controlled, &mut transform, PLAYER_KICK_OFF, Vec3::X);
    }
}

fn show_score(
    score: Res<Score>,
    mut boards: Query<&mut Text, (With<ScoreBoard>, Without<GoalBanner>)>,
    mut banners: Query<&mut Text, (With<GoalBanner>, Without<ScoreBoard>)>,
) {
    if !score.is_changed() {
        return;
    }
    for mut text in &mut boards {
        text.0 = format!("Chaos FC  {}  -  {}  Adversaires", score.home, score.away);
    }
    for mut text in &mut banners {
        text.0 = if score.celebrating.is_some() {
            "BUT !".to_owned()
        } else {
            String::new()
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_goal_needs_the_whole_ball_over_the_line() {
        let line = pitch::goal_line(1.0);
        assert_eq!(goal_for(Vec3::new(line, 0.11, 0.0)), None);
        assert_eq!(goal_for(Vec3::new(line + 0.1, 0.11, 0.0)), None);
        assert_eq!(
            goal_for(Vec3::new(line + 0.12, 0.11, 0.0)),
            Some(Team::Home)
        );
        assert_eq!(goal_for(Vec3::new(-line - 0.5, 1.0, 3.0)), Some(Team::Away));
    }

    #[test]
    fn wide_or_high_is_not_a_goal() {
        let x = pitch::goal_line(1.0) + 1.0;
        assert_eq!(goal_for(Vec3::new(x, 0.11, 4.0)), None);
        assert_eq!(goal_for(Vec3::new(x, 2.6, 0.0)), None);
    }
}
