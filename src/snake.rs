use crossterm::event::KeyCode;
use rand::Rng;

#[derive(Clone, Copy, PartialEq)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

impl Direction {
    fn opposite(&self) -> Direction {
        match self {
            Direction::Up => Direction::Down,
            Direction::Down => Direction::Up,
            Direction::Left => Direction::Right,
            Direction::Right => Direction::Left,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct Pos {
    pub x: i16,
    pub y: i16,
}

pub struct SnakeGame {
    pub width: u16,
    pub height: u16,
    pub snake: Vec<Pos>,
    pub food: Pos,
    pub direction: Direction,
    pub next_direction: Direction,
    pub score: u16,
    pub game_over: bool,
    pub ticks_per_move: u64,
    pub tick_count: u64,
}

impl SnakeGame {
    pub fn new(width: u16, height: u16) -> Self {
        let cx = width as i16 / 2;
        let cy = height as i16 / 2;
        let snake = vec![
            Pos { x: cx, y: cy },
            Pos { x: cx - 1, y: cy },
            Pos { x: cx - 2, y: cy },
        ];

        let mut game = Self {
            width,
            height,
            snake,
            food: Pos { x: 0, y: 0 },
            direction: Direction::Right,
            next_direction: Direction::Right,
            score: 0,
            game_over: false,
            ticks_per_move: 7, // ~112ms at 16ms poll
            tick_count: 0,
        };
        game.place_food();
        game
    }

    pub fn handle_key(&mut self, code: KeyCode) {
        let new_dir = match code {
            KeyCode::Up => Some(Direction::Up),
            KeyCode::Down => Some(Direction::Down),
            KeyCode::Left => Some(Direction::Left),
            KeyCode::Right => Some(Direction::Right),
            _ => None,
        };

        if let Some(dir) = new_dir {
            if dir != self.direction.opposite() {
                self.next_direction = dir;
            }
        }
    }

    pub fn tick(&mut self) {
        if self.game_over {
            return;
        }

        self.tick_count += 1;
        if self.tick_count % self.ticks_per_move != 0 {
            return;
        }

        self.direction = self.next_direction;

        let head = self.snake[0];
        let new_head = match self.direction {
            Direction::Up => Pos { x: head.x, y: head.y - 1 },
            Direction::Down => Pos { x: head.x, y: head.y + 1 },
            Direction::Left => Pos { x: head.x - 1, y: head.y },
            Direction::Right => Pos { x: head.x + 1, y: head.y },
        };

        // Wall collision
        if new_head.x < 0
            || new_head.x >= self.width as i16
            || new_head.y < 0
            || new_head.y >= self.height as i16
        {
            self.game_over = true;
            return;
        }

        // Self collision
        if self.snake.iter().any(|p| *p == new_head) {
            self.game_over = true;
            return;
        }

        self.snake.insert(0, new_head);

        if new_head == self.food {
            self.score += 1;
            self.place_food();
            // Speed up slightly
            if self.ticks_per_move > 3 && self.score % 5 == 0 {
                self.ticks_per_move -= 1;
            }
        } else {
            self.snake.pop();
        }
    }

    fn place_food(&mut self) {
        let mut rng = rand::rng();
        loop {
            let pos = Pos {
                x: rng.random_range(0..self.width as i16),
                y: rng.random_range(0..self.height as i16),
            };
            if !self.snake.contains(&pos) {
                self.food = pos;
                break;
            }
        }
    }
}
