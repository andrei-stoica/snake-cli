use std::collections::VecDeque;
use std::io::stdout;
use std::sync::mpsc::{self, Receiver};
use std::thread;

use crossterm::cursor::{self, MoveTo};
use crossterm::event::{read, Event, KeyCode};
use crossterm::execute;
use crossterm::style::{Print, Stylize};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, window_size, Clear, ClearType, EnterAlternateScreen,
    LeaveAlternateScreen,
};

use game_loop::game_loop;
use rand::random;

fn main() {
    let g = Game::new();
    execute!(stdout(), EnterAlternateScreen, cursor::Hide);
    enable_raw_mode();
    g.render_board();
    let game = game_loop(
        g,
        4,
        0.16,
        |g| {
            let mut inputQueue = Vec::<Direction>::new();
            while let Ok(input) = g.game.input.try_recv() {
                if let Ok(direction) = Direction::try_from(input) {
                    inputQueue.push(direction.clone());
                    match direction {
                        Direction::Up if !matches!(g.game.direction, Direction::Down) => {
                            g.game.direction = direction
                        }
                        Direction::Down if !matches!(g.game.direction, Direction::Up) => {
                            g.game.direction = direction
                        }
                        Direction::Right if !matches!(g.game.direction, Direction::Left) => {
                            g.game.direction = direction
                        }
                        Direction::Left if !matches!(g.game.direction, Direction::Right) => {
                            g.game.direction = direction
                        }
                        _ => {}
                    };
                } else {
                    g.exit();
                }
            }
            let update_step = g.game.get_step(&mut inputQueue);
            if let Err(gg) = g.game.update(update_step) {
                g.exit();
            }
        },
        |g| {
            g.game.render();
        },
    );
    disable_raw_mode();
    execute!(stdout(), LeaveAlternateScreen, cursor::Show);
}

#[derive(Clone, Debug)]
enum BoardState {
    Empty,
    Apple,
    Snake,
}

#[derive(Debug, Clone, Copy)]
enum Direction {
    Left,
    Up,
    Right,
    Down,
}

#[derive(Debug, Clone, Copy)]
enum Input {
    Left,
    Up,
    Right,
    Down,
    Exit,
}

#[derive(Debug, Clone, Copy)]
enum Step {
    Turn(Direction),
    MoveAndTurn {
        step_direction: Direction,
        new_direction: Direction,
    },
}

#[derive(Debug, Clone, Copy)]
enum GameOverState {
    SnakeBite,
    OutOfBounds,
}

#[derive(Debug)]
enum InputError {
    Exit,
}

impl TryFrom<Input> for Direction {
    type Error = InputError;

    fn try_from(value: Input) -> Result<Self, Self::Error> {
        match value {
            Input::Up => Ok(Direction::Up),
            Input::Right => Ok(Direction::Right),
            Input::Down => Ok(Direction::Down),
            Input::Left => Ok(Direction::Left),
            Input::Exit => Err(Self::Error::Exit),
        }
    }
}

impl Direction {
    fn valid_direction(cur_direction: &Direction, new_direction: &Direction) -> bool {
        match new_direction {
            Direction::Up if matches!(cur_direction, Direction::Down) => false,
            Direction::Down if matches!(cur_direction, Direction::Up) => false,
            Direction::Right if matches!(cur_direction, Direction::Left) => false,
            Direction::Left if matches!(cur_direction, Direction::Right) => false,
            _ => true,
        }
    }
}

#[derive(Debug)]
struct Game {
    apple_pos: (usize, usize),
    apple_old: (usize, usize),
    board_size: (usize, usize),
    snake: VecDeque<(usize, usize)>,
    snake_old: VecDeque<(usize, usize)>,
    direction: Direction,
    input: Receiver<Input>,
}

impl Game {
    fn new() -> Self {
        let mut snake = VecDeque::new();
        for n in 0..5 {
            snake.push_back((0, n));
        }

        let (tx, rx) = mpsc::channel::<Input>();

        thread::spawn(move || loop {
            loop {
                let event = read().unwrap();
                let _send_res = if event == Event::Key(KeyCode::Char('w').into())
                    || event == Event::Key(KeyCode::Up.into())
                {
                    tx.send(Input::Up)
                } else if event == Event::Key(KeyCode::Char('d').into())
                    || event == Event::Key(KeyCode::Right.into())
                {
                    tx.send(Input::Right)
                } else if event == Event::Key(KeyCode::Char('s').into())
                    || event == Event::Key(KeyCode::Down.into())
                {
                    tx.send(Input::Down)
                } else if event == Event::Key(KeyCode::Char('a').into())
                    || event == Event::Key(KeyCode::Left.into())
                {
                    tx.send(Input::Left)
                } else if event == Event::Key(KeyCode::Char('q').into()) {
                    tx.send(Input::Exit)
                } else {
                    Ok(())
                };
                //print!("{:?}", event);
            }
        });
        let board_size = window_size().map_or((20, 40), |w_size| {
            ((w_size.rows - 2).into(), (w_size.columns - 2).into())
        });

        Game {
            apple_pos: Self::gen_apple(board_size.0, board_size.1),
            apple_old: Self::gen_apple(board_size.0, board_size.1),
            board_size,
            snake_old: snake.clone(),
            snake,
            direction: Direction::Right,
            input: rx,
        }
    }

    fn get_step(&self, input: &Vec<Direction>) -> Step {
        match input.len() {
            1 if Direction::valid_direction(&self.direction, &input[0]) => Step::Turn(input[0]),
            2.. if Direction::valid_direction(&self.direction, &input[input.len() - 2])
                && Direction::valid_direction(&input[input.len() - 2], &input[input.len() - 1]) =>
            {
                Step::MoveAndTurn {
                    step_direction: input[input.len() - 2],
                    new_direction: input[input.len() - 1],
                }
            }
            _ => Step::Turn(self.direction),
        }
    }

    fn advance(&mut self) -> Result<(), GameOverState> {
        let next_pos = self.next_pos()?;

        match self.check_pos(next_pos)? {
            BoardState::Apple => {
                self.new_apple();
            }
            BoardState::Empty => {
                self.snake.pop_front();
            }
            BoardState::Snake => unreachable!("Snake state returned"),
        }
        self.snake.push_back(next_pos);

        std::thread::sleep(std::time::Duration::from_millis(250));
        Ok(())
    }

    fn update(&mut self, step: Step) -> Result<(), GameOverState> {
        match step {
            Step::Turn(direction) => {
                self.direction = direction;
                self.advance()?;
            }
            Step::MoveAndTurn {
                step_direction,
                new_direction,
            } => {
                self.direction = step_direction;
                self.advance()?;
                self.direction = new_direction;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
        Ok(())
    }

    fn gen_apple(max_x: usize, max_y: usize) -> (usize, usize) {
        let x = random::<usize>() % max_x;
        let y = random::<usize>() % max_y;

        (x, y)
    }

    fn new_apple(&mut self) {
        self.apple_pos = Self::gen_apple(self.board_size.0, self.board_size.1);
    }

    fn next_pos(&self) -> Result<(usize, usize), GameOverState> {
        let head = self.snake.back().unwrap();
        match self.direction {
            Direction::Up => Ok((
                head.0.checked_sub(1).ok_or(GameOverState::OutOfBounds)?,
                head.1,
            )),
            Direction::Left => Ok((
                head.0,
                head.1.checked_sub(1).ok_or(GameOverState::OutOfBounds)?,
            )),
            Direction::Down => Ok((head.0 + 1, head.1)),
            Direction::Right => Ok((head.0, head.1 + 1)),
        }
    }

    fn check_pos(&self, pos: (usize, usize)) -> Result<BoardState, GameOverState> {
        if pos.1 >= self.board_size.1 || pos.0 >= self.board_size.0 {
            return Err(GameOverState::OutOfBounds);
        } else if pos.1 == self.apple_pos.1 && pos.0 == self.apple_pos.0 {
            return Ok(BoardState::Apple);
        }
        for snake_pos in &self.snake {
            if pos.0 == snake_pos.0 && pos.1 == snake_pos.1 {
                return Err(GameOverState::SnakeBite);
            }
        }
        return Ok(BoardState::Empty);
    }

    fn render_board(&self) {
        execute!(stdout(), Clear(ClearType::All), Print("\r\n-".reset()));
        for _ in 0..self.board_size.1 {
            execute!(stdout(), Print("-"));
        }
        execute!(stdout(), Print("-\r\n"));

        for x in 0..self.board_size.0 {
            execute!(stdout(), Print("|".reset()));
            for y in 0..self.board_size.1 {
                execute!(stdout(), Print(" ".reset()));
            }
            execute!(stdout(), Print("|\r\n".reset()));
        }

        for _ in 0..self.board_size.1 + 2 {
            execute!(stdout(), Print("-"));
        }
    }

    fn render(&mut self) {
        self.snake_old.iter().for_each(|pos| {
            execute!(
                stdout(),
                MoveTo(1 + pos.1 as u16, 1 + pos.0 as u16),
                Print(" ".reset())
            );
        });

        self.snake.iter().for_each(|pos| {
            execute!(
                stdout(),
                MoveTo(1 + pos.1 as u16, 1 + pos.0 as u16),
                Print("S".green())
            );
        });

        execute!(
            stdout(),
            MoveTo(1 + self.apple_old.1 as u16, 1 + self.apple_old.0 as u16),
            Print(" ".reset())
        );
        execute!(
            stdout(),
            MoveTo(1 + self.apple_pos.1 as u16, 1 + self.apple_pos.0 as u16),
            Print("A".red())
        );

        execute!(
            stdout(),
            MoveTo(self.board_size.1 as u16 + 1, self.board_size.0 as u16 + 1)
        );

        self.snake_old = self.snake.clone();
        self.apple_old = self.apple_pos.clone();
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
}
