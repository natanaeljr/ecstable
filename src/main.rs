#![allow(dead_code)]

use legion::{World, Entity, IntoQuery, EntityStore};
use csv::Reader;
use crossterm::{
    cursor, event, execute, queue, style,
    terminal::{self, ClearType},
};
use std::io::Write;
use crossterm::event::{Event, KeyCode, KeyModifiers, MouseEventKind, MouseButton};
use itertools::Itertools;
use necst::Registry;

fn main() {
    let table = read_table();
    let mut world = World::default();
    table_to_ecs(&mut world, &table);

    // print_table_raw(&table);
    // print_table_ecs(&world);
    // print_table_ecs_hierarchical(&world);
    run_tui(&mut world);
}

fn run_tui(world: &mut World) {
    // let mut stdout = BufWriter::new(std::io::stdout()); // BufWriter decreases flickering
    let mut stdout = std::io::stdout();

    terminal::enable_raw_mode().unwrap();
    execute!(
        stdout,
        terminal::EnterAlternateScreen,
        // terminal::SetTitle("ecstable"),
        terminal::Clear(ClearType::All),
        event::EnableMouseCapture,
        // cursor::Hide
    )
        .unwrap();

    tui_loop(&mut stdout, world);

    execute!(
        stdout,
        terminal::Clear(ClearType::All),
        style::ResetColor,
        // cursor::Show,
        event::DisableMouseCapture,
        // terminal::SetTitle(""),
        terminal::LeaveAlternateScreen,
    )
        .unwrap();
    stdout.flush().unwrap();
    terminal::disable_raw_mode().unwrap();
}

fn tui_loop<W: std::io::Write>(stdout: &mut W, world: &mut World) {
    let mut canvas = Canvas::default();
    canvas.resize(terminal::size().unwrap());

    loop {
        // Rendering
        queue!(stdout,
            style::ResetColor,
            terminal::Clear(ClearType::All),
            cursor::MoveTo(0,0),
        ).unwrap();

        draw(stdout, &world, &mut canvas);
        canvas.print(&mut std::io::stderr());
        stdout.flush().unwrap();

        // Event handling
        let mut quit = false;
        event_loop(&mut quit, world, &mut canvas);
        if quit {
            break;
        }
    }
}

#[derive(Default, Debug)]
struct LeftClicked();

#[derive(Default, Debug)]
struct LeftReleased();

fn event_loop(quit: &mut bool, world: &mut World, canvas: &mut Canvas) {
    loop {
        match event::read().unwrap() {
            Event::Key(key) => {
                if key.modifiers == KeyModifiers::empty() {
                    match key.code {
                        KeyCode::Char('q') => {
                            *quit = true;
                            break;
                        }
                        _ => {}
                    }
                }
            }
            Event::Mouse(mouse) => {
                match mouse.kind {
                    MouseEventKind::Down(button) => {
                        if button == MouseButton::Left {
                            let entity = canvas.matrix[mouse.row as usize][mouse.column as usize].as_ref();
                            eprintln!("clicked on {:?}", entity);
                            if let Some(entity) = entity {
                                let mut entry = world.entry(*entity).unwrap();
                                entry.add_component(LeftClicked());
                                break;
                            }
                        }
                    }
                    MouseEventKind::Up(button) => {
                        if button == MouseButton::Left {
                            let entity = canvas.matrix[mouse.row as usize][mouse.column as usize].as_ref();
                            eprintln!("released on {:?}", entity);
                            if let Some(entity) = entity {
                                let mut entry = world.entry(*entity).unwrap();
                                entry.add_component(LeftReleased());
                            }
                        }
                    }
                    _ => {}
                }

                let mut should_break = false;
                let mut left_clicked_entt = None;
                let mut query = <(Entity, &Cell, &Parent, &LeftClicked)>::query();
                for (entt, cell, parent, left_clicked) in query.iter(world) {
                    left_clicked_entt = Some((entt.clone(), parent.0.clone()));
                    break;
                }
                let mut left_released_entt = None;
                let mut query = <(Entity, &Cell, &Parent, &LeftReleased)>::query();
                for (entt, cell, parent, left_released) in query.iter(world) {
                    left_released_entt = Some((entt.clone(), parent.0.clone()));
                    break;
                }
                if left_clicked_entt.is_some() && left_released_entt.is_some() {
                    let clicked_entt = left_clicked_entt.unwrap();
                    let released_entt = left_released_entt.unwrap();
                    if clicked_entt.1 == released_entt.1 {
                        let mut parent_entry = world.entry_mut(released_entt.1).unwrap();
                        let mut row = parent_entry.get_component_mut::<Row>().unwrap();
                        let clicked_entt = row.cells.iter().find_position(|entt| *entt == &clicked_entt.0).unwrap().0;
                        let released_entt = row.cells.iter().find_position(|entt| *entt == &released_entt.0).unwrap().0;
                        row.cells.swap(clicked_entt, released_entt);
                        should_break = true;
                    }
                    {
                        let mut clicked_entry = world.entry(clicked_entt.0).unwrap();
                        clicked_entry.remove_component::<LeftClicked>();
                    }
                    {
                        let mut released_entry = world.entry(released_entt.0).unwrap();
                        released_entry.remove_component::<LeftReleased>();
                    }
                }
                if should_break {
                    break;
                }
            }
            Event::Resize(cols, rows) => {
                canvas.resize((cols, rows));
                break;
            }
        }
    }
}

fn draw<W: std::io::Write>(out: &mut W, world: &World, canvas: &mut Canvas) {
    // print_table_raw(&table);
    // print_table_ecs(&world);
    print_table_ecs_hierarchical(out, world, canvas);
}

// TODO: Canvas must implement Write, but how to pass the Entity?
//
// Primary idea is to detect witch entity was clicked on, similar to game-mouse-picking with framebuffer.
// That's why each cell received the entity ID.
//
// Other idea is that the Canvas holds properties for each cell, like the color, modifiers, the character, etc,
// so that we can compare and search for altered cells to find out which ones should be updated
// and generate cursor jumps to update only those cells.
//
// Maybe add depth? That could be complicated though.

#[derive(Debug, Default)]
struct Canvas {
    pub matrix: Vec<Vec<Option<Entity>>>, // TODO: transform into a single vector
}

impl Canvas {
    fn paint(&mut self, entt: Entity, len: usize) {
        let (col, row) = cursor::position().unwrap();
        let (max_col, _max_row) = terminal::size().unwrap();
        let len = std::cmp::min(len, max_col as usize);
        for c in 0..len {
            self.matrix[row as usize][col as usize + c] = Some(entt);
        }
    }

    // TODO Methods:
    // paint_with_rect ? an entity clickable area might be larger that what it writes on
    // paint_row ?
    // paint_with_depth ?,

    fn resize(&mut self, (col, row): (u16, u16)) {
        // TODO: reuse current cells, no need to reallocate everything, only delete what's necessary
        self.matrix.clear();
        for r in 0..row {
            self.matrix.push(Vec::new());
            for c in 0..col {
                self.matrix.last_mut().unwrap().push(None);
            }
        }
    }

    fn print<W: std::io::Write>(&self, out: &mut W) {
        for row in &self.matrix {
            for cell in row {
                write!(out, "{}", if cell.is_some() { "1" } else { "0" });
            }
            writeln!(out);
        }
    }
}

fn print_table_ecs_hierarchical_n<W: std::io::Write>(out: &mut W, registry: &Registry, canvas: &mut Canvas) {
    for (_entt, (table, )) in registry.view_all::<(TableN, )>() {
        writeln!(out);
        for header_entt in &table.headers {
            let header = registry.get::<Header>(*header_entt).unwrap();
            let selected = registry.get::<Selected>(*header_entt).is_some();
            let data = format!("{}{}{}, ", if selected { "[" } else { "" }, header.0, if selected { "]" } else { "" });
            write!(out, "{}", data);
        }
        writeln!(out);
        for row_entt in &table.rows {
            let row = registry.get::<RowN>(*row_entt).unwrap();
            for cell_entt in &row.cells {
                let cell = registry.get::<Cell>(*cell_entt).unwrap();
                let selected = registry.get::<Selected>(*cell_entt).is_some();
                let clicked = registry.get::<LeftClicked>(*cell_entt).is_some();
                let data = format!("{}{}{}, ", if selected || clicked { "[" } else { "" }, cell.0, if selected || clicked { "]" } else { "" });
                write!(out, "{}", data);
            }
            writeln!(out);
        }
        writeln!(out);
    }
}

fn print_table_ecs_hierarchical<W: std::io::Write>(out: &mut W, world: &World, canvas: &mut Canvas) {
    let mut tables = <(&Table, )>::query();
    for (table, ) in tables.iter(world) {
        writeln!(out);

        for header_entt in &table.headers {
            let header_entry = world.entry_ref(*header_entt).unwrap();
            let header = header_entry.get_component::<Header>().unwrap();
            let selected = header_entry.get_component::<Selected>().is_ok();
            let data = format!("{}{}{}, ", if selected { "[" } else { "" }, header.0, if selected { "]" } else { "" });
            canvas.paint(*header_entt, data.len());
            write!(out, "{}", data);
        }
        writeln!(out);

        for row_entt in &table.rows {
            let row_entry = world.entry_ref(*row_entt).unwrap();
            let row = row_entry.get_component::<Row>().unwrap();
            for cell_entt in &row.cells {
                let cell_entry = world.entry_ref(*cell_entt).unwrap();
                let cell = cell_entry.get_component::<Cell>().unwrap();
                let selected = cell_entry.get_component::<Selected>().is_ok();
                let clicked = cell_entry.get_component::<LeftClicked>().is_ok();
                let data = format!("{}{}{}, ", if selected || clicked { "[" } else { "" }, cell.0, if selected || clicked { "]" } else { "" });
                canvas.paint(*cell_entt, data.len());
                write!(out, "{}", data);
            }
            writeln!(out);
        }

        writeln!(out);
    }
}

fn print_table_ecs(world: &World) {
    let mut tables = <(Entity, &Table, )>::query();
    for (table_entt, _table) in tables.iter(world) {
        println!();

        let mut headers = <(Entity, &Header, &Parent)>::query();
        for (_header_entt, header, header_parent) in headers.iter(world) {
            if &header_parent.0 == table_entt {
                print!("{}, ", header.0);
            }
        }
        println!();

        let mut rows = <(Entity, &Row, &Parent)>::query();
        for (row_entt, _row, row_parent) in rows.iter(world) {
            if &row_parent.0 == table_entt {
                let mut cells = <(Entity, &Cell, &Parent)>::query();
                for (_cell_entt, cell, cell_parent) in cells.iter(world) {
                    if &cell_parent.0 == row_entt {
                        print!("{}, ", cell.0);
                    }
                }
                println!();
            }
        }
        println!();
    }
}

#[derive(Debug, Default)]
struct Table {
    headers: Vec<Entity>,
    rows: Vec<Entity>,
}

#[derive(Debug, Default)]
struct TableN {
    headers: Vec<necst::Entity>,
    rows: Vec<necst::Entity>,
}

#[derive(Debug, Default)]
struct Header(String);

#[derive(Debug, Default)]
struct Row {
    cells: Vec<Entity>
}

#[derive(Debug, Default)]
struct RowN {
    cells: Vec<necst::Entity>
}

#[derive(Debug, Default)]
struct Cell(String);

#[derive(Debug)]
struct Parent(Entity);

#[derive(Debug)]
struct Selected();

#[derive(Debug)]
struct ParentN(necst::Entity);

fn table_to_ecs_new(table_data: &TableData) {
    let mut registry = Registry::new();
    let table = registry.create_with((TableN::default(), ));
    for header_data in &table_data.headers {
        let header = registry.create_with((Header(header_data.clone()), ParentN(table)));
        registry.patch::<TableN>(table).with(|table| table.headers.push(header));
    }
    for row_data in &table_data.rows {
        let row = registry.create_with((RowN::default(), ParentN(table)));
        registry.patch::<TableN>(table).with(|table| table.rows.push(row));
        for cell_data in row_data {
            let cell = registry.create_with((Cell(cell_data.clone()), ParentN(row)));
            registry.patch::<RowN>(row).with(|row| row.cells.push(cell));
        }
    }
}

fn table_to_ecs(world: &mut World, table_data: &TableData) {
    let table_entt = world.push((Table::default(), ));
    let mut i = 0;
    for header in &table_data.headers {
        let header_entt = world.push((Header(header.clone()), Parent(table_entt)));
        {
            let mut table_entry = world.entry(table_entt).unwrap();
            let table = table_entry.get_component_mut::<Table>().unwrap();
            table.headers.push(header_entt);
        }
        if i == 1 {
            let mut header_entry = world.entry(header_entt).unwrap();
            header_entry.add_component(Selected());
        }
        i += 1;
    }
    for row_data in &table_data.rows {
        let row_entt = world.push((Row::default(), Parent(table_entt)));
        {
            let mut table_entry = world.entry(table_entt).unwrap();
            let table = table_entry.get_component_mut::<Table>().unwrap();
            table.rows.push(row_entt);
        }
        let mut c = 0;
        for cell_data in row_data {
            let cell_entt = world.push((Cell(cell_data.clone()), Parent(row_entt)));
            {
                let mut row_entry = world.entry(row_entt).unwrap();
                let row = row_entry.get_component_mut::<Row>().unwrap();
                row.cells.push(cell_entt);
            }
            if c == 1 {
                let mut cell_entry = world.entry(cell_entt).unwrap();
                cell_entry.add_component(Selected());
            }
            c += 1;
        }
    }
}

#[derive(Debug, Default)]
struct TableData {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
}

fn read_table() -> TableData {
    let mut table = TableData::default();
    let mut reader = Reader::from_path("table.csv").unwrap();
    for headers in reader.headers() {
        for header in headers {
            table.headers.push(header.to_string());
        }
    }
    for record in reader.records().map(|r| r.unwrap()) {
        table.rows.push(Vec::new());
        for cell in &record {
            table.rows.last_mut().unwrap().push(cell.to_string());
        }
    }
    table
}

fn print_table_raw(table: &TableData) {
    println!();
    for header in &table.headers {
        print!("{}, ", header);
    }
    println!();
    for row in &table.rows {
        for cell in row {
            print!("{}, ", cell);
        }
        println!()
    }
    println!();
}