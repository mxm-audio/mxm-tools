use super::*;

pub fn living_room() -> Result<Space, Error> {
    let walls = surface(
        "walls",
        &[
            part(m::PLASTERBOARD, 0.30, 0.0),
            part(m::PLASTERBOARD, 0.25, 0.2),
            part(m::WOOD_LINING, 0.15, 0.0),
            part(m::GLASS, 0.25, 0.0),
            part(m::VELOUR, 0.05, 0.1),
        ],
    )?;
    let floor = surface(
        "floor",
        &[
            part(m::WOOD_FLOOR, 0.55, 0.0),
            part(m::CARPET, 0.25, 0.01),
            part(m::SEATS, 0.20, 0.45),
        ],
    )?;
    let ceiling = surface("ceiling", &[part(m::PLASTERBOARD, 1.0, 0.0)])?;
    Ok(Space {
        room: Room::shoebox(v(5.6, 4.3, 2.6), box_faces(&walls, floor, ceiling))?,
        air: Air::standard(),
        sources: sources(v(1.0, 2.0, 1.2), v(1.0, 1.1, 1.2), v(1.0, 2.9, 1.2)),
        positions: positions(v(2.6, 1.8, 1.2), v(4.7, 2.5, 1.2)),
        non_diffuse: false,
        seam_hz: None,
        max_time_s: 2.0,
        occupancy: "unoccupied, furnished",
        reference: gated(
            Decay::T30,
            octaves([
                (0.52, 0.62),
                (0.62, 0.67),
                (0.70, 0.74),
                (0.67, 0.84),
                (0.63, 0.83),
                (0.67, 0.80),
            ]),
            "Hoshi Table 7: three furnished living-dining rooms, 74–175 m³",
        ),
        notes: vec![
            "panel walls and ceiling on studs, as in the measured rooms, absorb at 125–250 Hz",
            "the sofa and chairs are upholstered seating over 20 % of the floor; wood panelling on 15 % of the walls, drapes on 5 %",
        ],
    })
}

pub fn tiled_bathroom() -> Result<Space, Error> {
    let walls = surface(
        "walls",
        &[
            part(m::TILE, 0.79, 0.0),
            part(m::WOOD_LINING, 0.08, 0.0),
            part(m::VELOUR, 0.08, 0.05),
            part(m::GLASS, 0.05, 0.0),
        ],
    )?;
    let floor = surface(
        "floor",
        &[part(m::TILE, 0.85, 0.0), part(m::HARD, 0.15, 0.3)],
    )?;
    let ceiling = surface("ceiling", &[part(m::PLASTER, 1.0, 0.0)])?;
    Ok(Space {
        room: Room::shoebox(v(2.5, 2.0, 2.5), box_faces(&walls, floor, ceiling))?,
        air: Air::standard(),
        sources: sources(v(0.8, 1.0, 1.5), v(0.8, 0.6, 1.5), v(0.8, 1.4, 1.5)),
        positions: positions(v(1.5, 0.8, 1.4), v(2.1, 1.5, 1.6)),
        non_diffuse: false,
        seam_hz: None,
        max_time_s: 2.5,
        occupancy: "unoccupied, towels and a door",
        reference: reported(
            Decay::T30,
            mid((0.6, 1.2)),
            "estimate (research notes §2): no published measurement",
        ),
        notes: vec![
            "the wood lining row stands for the door, velour for towels, the hard row for the bath",
        ],
    })
}

pub fn hard_small_room() -> Result<Space, Error> {
    let walls = surface(
        "walls",
        &[
            part(m::PLASTERBOARD, 0.45, 0.2),
            part(m::GLASS, 0.30, 0.0),
            part(m::WOOD_LINING, 0.20, 0.3),
            part(m::VELOUR, 0.05, 0.1),
        ],
    )?;
    let floor = surface("floor", &[part(m::HARD, 0.5, 0.0), part(m::PEWS, 0.5, 0.3)])?;
    let ceiling = surface(
        "ceiling",
        &[
            part(m::PLASTERBOARD, 0.25, 0.0),
            part(m::GLASS_WOOL, 0.75, 0.0),
        ],
    )?;
    Ok(Space {
        room: Room::shoebox(v(9.0, 7.0, 3.0), box_faces(&walls, floor, ceiling))?,
        air: Air::standard(),
        sources: sources(v(1.5, 3.3, 1.6), v(1.5, 2.3, 1.6), v(1.5, 4.3, 1.6)),
        positions: positions(v(3.8, 3.0, 1.2), v(7.6, 4.4, 1.2)),
        non_diffuse: false,
        seam_hz: None,
        max_time_s: 2.0,
        occupancy: "unoccupied, desks and chairs",
        reference: gated(
            Decay::T20,
            Target::Max {
                first: 1,
                last: 4,
                range: (0.34, 0.82),
            },
            "Keränen Table 3: 62 enclosed classrooms, maximum T20 over 250 Hz–2 kHz",
        ),
        notes: vec![
            "desks and chairs are the uncushioned pews row over half the floor",
            "absorbent tiles over 75 % of the ceiling, as the measured classrooms have; plasterboard partitions, curtains at the windows; cupboards and boards by depth",
        ],
    })
}

pub fn kitchen() -> Result<Space, Error> {
    let size = v(4.2, 3.4, 2.5);
    let walls = surface(
        "walls",
        &[
            part(m::PLASTER, 0.30, 0.0),
            part(m::TILE, 0.15, 0.0),
            part(m::WOOD_LINING, 0.30, 0.3),
            part(m::GLASS, 0.10, 0.0),
            part(m::CURTAIN, 0.15, 0.2),
        ],
    )?;
    let floor = surface("floor", &[part(m::TILE, 1.0, 0.0)])?;
    let ceiling = surface("ceiling", &[part(m::PLASTERBOARD, 1.0, 0.0)])?;
    let units = surface("units and worktop", &[part(m::WOOD_LINING, 1.0, 0.1)])?;
    let table = surface(
        "table and chairs",
        &[
            part(m::WOODEN_CHAIRS, 0.25, 0.75),
            part(m::WOOD_LINING, 0.75, 0.0),
        ],
    )?;
    let bench = surface(
        "cushioned bench",
        &[part(m::SEATS, 0.25, 0.5), part(m::WOOD_LINING, 0.75, 0.0)],
    )?;
    let solids = [
        block((0.3, 3.9), (0.12, 0.72), (0.1, 0.9), &units),
        block((1.6, 2.6), (1.9, 2.9), (0.1, 0.75), &table),
        block((1.6, 2.6), (3.0, 3.28), (0.1, 0.5), &bench),
    ];
    Ok(space(
        boxed(size, &walls, floor, ceiling)?.with_solids(&solids)?,
        layout(size),
        2.0,
        "unoccupied, a table and chairs",
        reported(
            Decay::T30,
            Target::Octaves([None, None, None, Some((0.68, 0.68)), None, None]),
            "Jackson & Leventhall via Hoshi p. 1 (secondary): 50 kitchens, 0.68 s at 1 kHz",
        ),
        vec![
            "a run of units, a table with chairs and a cushioned bench as solids; a tiled splashback the tile row, blinds curtain",
        ],
    ))
}

pub fn bedroom() -> Result<Space, Error> {
    let size = v(4.4, 3.8, 2.6);
    let walls = surface(
        "walls",
        &[
            part(m::PLASTERBOARD, 0.55, 0.0),
            part(m::GLASS, 0.15, 0.0),
            part(m::CURTAIN, 0.15, 0.2),
            part(m::WOOD_LINING, 0.15, 0.1),
        ],
    )?;
    let floor = surface(
        "floor",
        &[
            part(m::CARPET, 0.867, 0.01),
            part(m::WOOD_FLOOR, 0.133, 0.0),
        ],
    )?;
    let ceiling = surface("ceiling", &[part(m::PLASTERBOARD, 1.0, 0.0)])?;
    let bed = surface(
        "bed",
        &[part(m::SEATS, 0.25, 0.5), part(m::WOOD_LINING, 0.75, 0.0)],
    )?;
    let wardrobe = surface("wardrobe", &[part(m::WOOD_LINING, 1.0, 0.0)])?;
    let solids = [
        block((2.6, 4.2), (0.2, 1.2), (0.12, 0.6), &bed),
        block((0.2, 0.75), (2.9, 3.6), (0.12, 2.1), &wardrobe),
    ];
    Ok(space(
        boxed(size, &walls, floor, ceiling)?.with_solids(&solids)?,
        layout(size),
        1.5,
        "unoccupied, a made bed",
        reported(
            Decay::T30,
            octaves([
                (0.34, 0.38),
                (0.35, 0.44),
                (0.40, 0.51),
                (0.38, 0.48),
                (0.36, 0.46),
                (0.37, 0.44),
            ]),
            "Hoshi Table 7: three furnished bedrooms of 42–66 m³",
        ),
        vec![
            "a bed and a wardrobe as solids, the bed a quarter upholstery to three quarters frame; the floor is carpet and boards only",
        ],
    ))
}

pub fn narrow_hallway() -> Result<Space, Error> {
    let size = v(6.0, 1.3, 2.5);
    let walls = surface(
        "walls",
        &[
            part(m::PLASTER, 0.70, 0.0),
            part(m::WOOD_LINING, 0.20, 0.0),
            part(m::GLASS, 0.05, 0.0),
            part(m::VELOUR, 0.05, 0.3),
        ],
    )?;
    let floor = surface(
        "floor",
        &[
            part(m::WOOD_FLOOR, 0.85, 0.0),
            part(m::CARPET_THIN, 0.15, 0.01),
        ],
    )?;
    let ceiling = surface("ceiling", &[part(m::PLASTERBOARD, 1.0, 0.0)])?;
    Ok(space(
        boxed(size, &walls, floor, ceiling)?,
        layout(size),
        2.5,
        "empty, doors closed",
        reported(
            Decay::T30,
            mid((0.6, 1.0)),
            "estimate: no published measurement",
        ),
        vec![
            "plaster walls 1.3 m apart, the class's flutter; doors are the wood lining row, coats velour, a runner the thin carpet",
        ],
    ))
}

pub fn walk_in_closet() -> Result<Space, Error> {
    let size = v(1.8, 1.4, 2.4);
    let walls = surface(
        "walls",
        &[
            part(m::VELOUR, 0.55, 0.5),
            part(m::WOOD_LINING, 0.35, 0.3),
            part(m::PLASTERBOARD, 0.10, 0.0),
        ],
    )?;
    let floor = surface("floor", &[part(m::CARPET, 1.0, 0.01)])?;
    let ceiling = surface("ceiling", &[part(m::PLASTERBOARD, 1.0, 0.0)])?;
    Ok(space(
        boxed(size, &walls, floor, ceiling)?,
        layout(size),
        0.8,
        "full of hanging clothes",
        reported(
            Decay::T20,
            mid((0.08, 0.2)),
            "estimate (survey row 4): a tiny enclosure under 0.2 s",
        ),
        vec!["hanging clothes are the velour row, 0.5 m deep; shelves wood lining"],
    ))
}

pub fn shower_stall() -> Result<Space, Error> {
    let walls = surface(
        "walls",
        &[part(m::TILE, 0.91, 0.0), part(m::VELOUR, 0.09, 0.05)],
    )?;
    let floor = surface("floor", &[part(m::TILE, 1.0, 0.0)])?;
    let ceiling = surface("ceiling", &[part(m::PLASTER, 1.0, 0.0)])?;
    Ok(space(
        boxed(v(1.2, 1.0, 2.3), &walls, floor, ceiling)?,
        (
            sources(v(0.3, 0.5, 1.7), v(0.3, 0.25, 1.7), v(0.3, 0.75, 1.7)),
            positions(v(0.62, 0.4, 1.55), v(0.9, 0.62, 1.3)),
        ),
        2.0,
        "empty, a towel over the door",
        reported(
            Decay::T30,
            mid((0.4, 1.0)),
            "estimate: below the tiled bathroom's 0.6–1.2 s (survey row 2)",
        ),
        vec![
            "the glass door is the glazed tile row, small stiff panes as in the car cabin; the towel velour",
        ],
    ))
}

pub fn attic_room() -> Result<Space, Error> {
    let floor = surface(
        "floor",
        &[
            part(m::WOOD_FLOOR, 0.5, 0.0),
            part(m::CARPET, 0.3, 0.01),
            part(m::SEATS, 0.2, 0.5),
        ],
    )?;
    let ceiling = surface(
        "roof slope",
        &[
            part(m::WOOD_LINING, 0.7, 0.15),
            part(m::GLASS, 0.1, 0.0),
            part(m::PLASTERBOARD, 0.2, 0.0),
        ],
    )?;
    let walls = surface(
        "walls",
        &[
            part(m::PLASTERBOARD, 0.8, 0.0),
            part(m::WOOD_LINING, 0.1, 0.0),
            part(m::CURTAIN, 0.1, 0.2),
        ],
    )?;
    let plan = [(0.0, 0.0), (6.5, 0.0), (6.5, 5.0), (0.0, 5.0)];
    Ok(space(
        under_sloping_ceiling(&plan, (1.3, 0.0, 0.36), [floor, ceiling, walls])?,
        (
            sources(v(1.2, 3.2, 1.3), v(1.2, 2.2, 1.3), v(1.2, 4.2, 1.3)),
            positions(v(3.0, 3.0, 1.2), v(5.5, 3.6, 1.2)),
        ),
        2.0,
        "unoccupied, a sofa and a rug",
        reported(
            Decay::T30,
            mid((0.4, 0.7)),
            "estimate: no published measurement",
        ),
        vec![
            "a lean-to roof from 1.3 m to 3.1 m, boarded between rafters (0.15 m), with a skylight",
        ],
    ))
}

pub fn brick_cellar() -> Result<Space, Error> {
    const WIDTH: f64 = 4.5;
    const SPRING: f64 = 1.4;
    let floor = surface(
        "flagstones",
        &[part(m::SANDSTONE, 0.6, 0.0), part(m::HARD, 0.4, 0.0)],
    )?;
    let walls = surface(
        "walls",
        &[part(m::BRICK, 0.8, 0.0), part(m::WOOD_LINING, 0.2, 0.4)],
    )?;
    let vault = surface("barrel vault", &[part(m::BRICK, 1.0, 0.0)])?;
    // Cross-section in (y, z): the floor, a wall to the springing, a vault of six facets, the wall.
    let radius = WIDTH / 2.0;
    let mut section = vec![((0.0, 0.0), 0), ((WIDTH, 0.0), 1), ((WIDTH, SPRING), 2)];
    for i in 1..6 {
        let theta = PI * i as f64 / 6.0;
        section.push((
            (radius + radius * theta.cos(), SPRING + radius * theta.sin()),
            2,
        ));
    }
    section.push(((0.0, SPRING), 1));
    Ok(space(
        swept(Axis::X, &section, 9.0, 1, vec![floor, walls, vault])?,
        (
            sources(v(1.5, 2.25, 1.5), v(1.5, 1.25, 1.5), v(1.5, 3.25, 1.5)),
            positions(v(4.0, 2.0, 1.5), v(7.5, 2.6, 1.5)),
        ),
        3.0,
        "empty, a few racks",
        reported(
            Decay::T30,
            mid((0.8, 1.5)),
            "estimate: no published measurement",
        ),
        vec![
            "a barrel vault of six facets on 1.4 m walls; racks are the wood lining row on a fifth of the walls",
        ],
    ))
}

pub fn garage() -> Result<Space, Error> {
    let size = v(6.0, 5.5, 2.6);
    let walls = surface(
        "walls",
        &[
            part(m::BLOCK_PAINTED, 0.75, 0.0),
            part(m::HARD, 0.15, 0.05),
            part(m::WOOD_LINING, 0.10, 0.4),
        ],
    )?;
    let floor = surface("floor", &[part(m::CONCRETE_FLOOR, 1.0, 0.0)])?;
    let ceiling = surface(
        "ceiling",
        &[part(m::PLASTERBOARD, 0.5, 0.0), part(m::HARD, 0.5, 0.2)],
    )?;
    Ok(space(
        boxed(size, &walls, floor, ceiling)?,
        layout(size),
        3.5,
        "empty, shelves along one wall",
        reported(
            Decay::T30,
            mid((1.5, 2.5)),
            "estimate: no published measurement",
        ),
        vec![
            "no row for a steel door: the hard row stands for it; shelves are wood lining 0.4 m deep",
        ],
    ))
}

pub fn empty_flat() -> Result<Space, Error> {
    let size = v(5.6, 4.3, 2.6);
    let walls = surface(
        "walls",
        &[
            part(m::PLASTERBOARD, 0.45, 0.0),
            part(m::PLASTER, 0.30, 0.0),
            part(m::GLASS, 0.25, 0.0),
        ],
    )?;
    let floor = surface("floor", &[part(m::WOOD_FLOOR, 1.0, 0.0)])?;
    let ceiling = surface("ceiling", &[part(m::PLASTERBOARD, 1.0, 0.0)])?;
    Ok(space(
        boxed(size, &walls, floor, ceiling)?,
        layout(size),
        3.0,
        "unfurnished",
        reported(
            Decay::T30,
            mid((1.2, 1.8)),
            "estimate: longer than the furnished rooms' live outlier (research notes §1, 1.1–1.35 s at 500 Hz–1 kHz)",
        ),
        vec!["the living room's size, emptied: stud walls, a plaster party wall, bare boards"],
    ))
}

pub fn sauna() -> Result<Space, Error> {
    let size = v(2.3, 2.0, 2.2);
    let walls = surface(
        "walls",
        &[
            part(m::WOOD_LINING, 0.8, 0.05),
            part(m::WOOD_LINING, 0.2, 0.5),
        ],
    )?;
    let floor = surface(
        "floor",
        &[part(m::WOOD_FLOOR, 0.6, 0.0), part(m::TILE, 0.4, 0.0)],
    )?;
    let ceiling = surface("ceiling", &[part(m::WOOD_LINING, 1.0, 0.05)])?;
    Ok(space(
        boxed(size, &walls, floor, ceiling)?,
        layout(size),
        1.0,
        "empty",
        reported(
            Decay::T20,
            mid((0.3, 0.6)),
            "estimate: no published measurement",
        ),
        vec![
            "tongue-and-groove lining throughout; the benches are lining 0.5 m deep on a fifth of the walls",
        ],
    ))
}

pub fn meeting_room() -> Result<Space, Error> {
    let size = v(7.0, 4.5, 2.8);
    let walls = surface(
        "walls",
        &[
            part(m::PLASTERBOARD, 0.45, 0.0),
            part(m::GLASS, 0.30, 0.0),
            part(m::WOOD_LINING, 0.10, 0.1),
            part(m::CURTAIN, 0.15, 0.2),
        ],
    )?;
    let floor = surface("floor", &[part(m::CARPET, 1.0, 0.01)])?;
    let ceiling = surface("ceiling", &[part(m::PLASTERBOARD, 1.0, 0.0)])?;
    let table = surface(
        "meeting table",
        &[
            part(m::WOODEN_CHAIRS, 0.25, 0.0),
            part(m::WOOD_LINING, 0.75, 0.0),
        ],
    )?;
    let chair = surface(
        "office chairs",
        &[
            part(m::MEDIUM_SEATS, 0.25, 0.6),
            part(m::WOOD_LINING, 0.75, 0.0),
        ],
    )?;
    let mut solids = vec![block((2.2, 5.0), (2.2, 3.0), (0.15, 0.75), &table)];
    solids.extend(row_of_blocks(
        5,
        Axis::X,
        (2.1, 5.1),
        0.45,
        (1.55, 1.95),
        (0.15, 0.85),
        &chair,
    ));
    solids.extend(row_of_blocks(
        5,
        Axis::X,
        (2.1, 5.1),
        0.45,
        (3.25, 3.65),
        (0.15, 0.85),
        &chair,
    ));
    Ok(space(
        boxed(size, &walls, floor, ceiling)?.with_solids(&solids)?,
        layout(size),
        1.8,
        "unoccupied, a table and chairs",
        reported(
            Decay::T20,
            Target::Mean {
                first: 0,
                last: 5,
                range: (0.54, 0.61),
            },
            "SoundCam Table 7: one untreated conference room, broadband 0.54–0.61 s over ten microphones",
        ),
        vec![
            "untreated: a glass wall, stud partitions, blinds, carpet; a table and ten office chairs as solids, the floor carpet alone",
        ],
    ))
}

pub fn open_plan_office() -> Result<Space, Error> {
    let size = v(24.0, 16.0, 2.9);
    let walls = surface(
        "walls",
        &[
            part(m::PLASTERBOARD, 0.5, 1.0),
            part(m::GLASS, 0.25, 0.2),
            part(m::CURTAIN, 0.15, 0.2),
            part(m::WOOD_LINING, 0.1, 0.3),
        ],
    )?;
    let floor = surface("floor", &[part(m::CARPET, 1.0, 0.01)])?;
    let ceiling = surface(
        "ceiling",
        &[
            part(m::MINERAL_BOARD, 0.85, 0.0),
            part(m::PLASTERBOARD, 0.15, 0.0),
        ],
    )?;
    let desk = surface(
        "desk banks",
        &[
            part(m::WOODEN_CHAIRS, 0.25, 0.0),
            part(m::WOOD_LINING, 0.75, 0.0),
        ],
    )?;
    let screen = surface("fabric screens", &[part(m::VELOUR, 1.0, 0.05)])?;
    let mut solids = row_of_blocks(
        5,
        Axis::X,
        (2.2, 21.8),
        1.6,
        (2.0, 14.0),
        (0.15, 0.75),
        &desk,
    );
    solids.extend(row_of_blocks(
        5,
        Axis::X,
        (2.4, 21.6),
        1.2,
        (2.2, 6.0),
        (0.85, 1.35),
        &screen,
    ));
    solids.extend(row_of_blocks(
        5,
        Axis::X,
        (2.4, 21.6),
        1.2,
        (8.0, 13.8),
        (0.85, 1.35),
        &screen,
    ));
    Ok(space(
        boxed(size, &walls, floor, ceiling)?.with_solids(&solids)?,
        layout(size),
        1.5,
        "unoccupied desks",
        reported(
            Decay::T20,
            Target::Max {
                first: 1,
                last: 4,
                range: (0.44, 0.72),
            },
            "Keränen Table 3: 11 open learning spaces, a neighbour class; maximum T20 over 250 Hz–2 kHz",
        ),
        vec![
            "five desk banks and ten fabric screens as solids; absorbent ceiling tiles over 85 %, blinds curtain, columns and storage by depth",
        ],
    ))
}

pub fn school_corridor() -> Result<Space, Error> {
    let size = v(40.0, 2.4, 2.9);
    let walls = surface(
        "walls",
        &[
            part(m::BLOCK_PAINTED, 0.50, 0.3),
            part(m::PERFORATED_WOOD, 0.10, 0.05),
            part(m::HARD, 0.25, 0.3),
            part(m::WOOD_LINING, 0.10, 0.2),
            part(m::GLASS, 0.05, 0.0),
        ],
    )?;
    let floor = surface("terrazzo", &[part(m::CONCRETE_FLOOR, 1.0, 0.0)])?;
    let ceiling = surface(
        "ceiling",
        &[
            part(m::MINERAL_BOARD, 0.6, 0.0),
            part(m::PLASTERBOARD, 0.4, 0.0),
        ],
    )?;
    Ok(space(
        boxed(size, &walls, floor, ceiling)?,
        layout(size),
        3.0,
        "empty, doors closed",
        reported(
            Decay::T30,
            mid((0.8, 1.6)),
            "estimate: no published measurement",
        ),
        vec![
            "lockers (the hard row, 0.3 m) and door recesses (0.2 m) by depth, pinboards perforated wood on a tenth of the walls; absorbent tiles over 60 % of the ceiling",
        ],
    ))
}

pub fn library_reading_room() -> Result<Space, Error> {
    let size = v(30.0, 18.0, 7.0);
    let walls = surface(
        "walls",
        &[
            part(m::WOOD_LINING, 0.45, 0.6),
            part(m::PLASTER, 0.35, 0.6),
            part(m::GLASS, 0.20, 0.0),
        ],
    )?;
    let floor = surface("floor", &[part(m::CARPET, 1.0, 0.01)])?;
    let ceiling = surface(
        "ceiling",
        &[
            part(m::PLASTER, 0.55, 0.5),
            part(m::MINERAL_BOARD, 0.45, 0.0),
        ],
    )?;
    let stack = surface("book stacks", &[part(m::WOOD_LINING, 1.0, 0.3)])?;
    let table = surface(
        "reading tables",
        &[
            part(m::WOODEN_CHAIRS, 0.25, 0.0),
            part(m::WOOD_LINING, 0.75, 0.0),
        ],
    )?;
    let chair = surface(
        "reading chairs",
        &[
            part(m::MEDIUM_SEATS, 0.25, 0.6),
            part(m::WOOD_LINING, 0.75, 0.0),
        ],
    )?;
    let mut solids = vec![
        block((16.5, 17.5), (2.0, 16.0), (0.2, 2.4), &stack),
        block((19.0, 20.0), (2.0, 16.0), (0.2, 2.4), &stack),
        block((21.5, 22.5), (2.0, 16.0), (0.2, 2.4), &stack),
        block((25.0, 26.0), (2.0, 16.0), (0.2, 2.4), &stack),
        block((27.5, 28.5), (2.0, 16.0), (0.2, 2.4), &stack),
    ];
    solids.extend(row_of_blocks(
        3,
        Axis::X,
        (7.0, 14.5),
        2.2,
        (6.0, 12.0),
        (0.2, 0.8),
        &table,
    ));
    solids.extend(row_of_blocks(
        3,
        Axis::X,
        (7.0, 14.5),
        0.5,
        (4.9, 5.5),
        (0.2, 0.9),
        &chair,
    ));
    solids.extend(row_of_blocks(
        3,
        Axis::X,
        (7.0, 14.5),
        0.5,
        (12.5, 13.1),
        (0.2, 0.9),
        &chair,
    ));
    Ok(space(
        boxed(size, &walls, floor, ceiling)?.with_solids(&solids)?,
        layout(size),
        3.0,
        "unoccupied tables and reading chairs",
        reported(
            Decay::T30,
            mid((0.9, 1.4)),
            "estimate: no published measurement",
        ),
        vec![
            "five book stacks, three reading tables and six chairs as solids; no row for books, so a stack is wood lining 0.3 m rough",
        ],
    ))
}

pub fn museum_gallery() -> Result<Space, Error> {
    let size = v(40.0, 12.0, 8.0);
    let walls = surface(
        "walls",
        &[part(m::PLASTER, 0.8, 0.5), part(m::PLASTERBOARD, 0.2, 0.5)],
    )?;
    let floor = surface("floor", &[part(m::PARQUET, 1.0, 0.0)])?;
    let ceiling = surface(
        "laylight",
        &[part(m::GLASS_WOOL, 0.35, 0.8), part(m::PLASTER, 0.65, 0.8)],
    )?;
    let case = surface(
        "display cases",
        &[
            part(m::PLATE_GLASS, 0.7, 0.0),
            part(m::WOOD_LINING, 0.3, 0.0),
        ],
    )?;
    let bench = surface("benches", &[part(m::WOODEN_CHAIRS, 1.0, 0.0)])?;
    let mut solids = row_of_blocks(6, Axis::X, (3.4, 34.6), 1.2, (1.0, 2.2), (0.15, 2.0), &case);
    solids.extend(row_of_blocks(
        6,
        Axis::X,
        (3.4, 34.6),
        1.2,
        (9.8, 11.0),
        (0.15, 2.0),
        &case,
    ));
    solids.extend(row_of_blocks(
        4,
        Axis::X,
        (11.2, 36.8),
        1.6,
        (5.6, 6.4),
        (0.15, 0.5),
        &bench,
    ));
    Ok(space(
        boxed(size, &walls, floor, ceiling)?.with_solids(&solids)?,
        layout(size),
        10.0,
        "empty gallery",
        reported(
            Decay::T30,
            mid((2.5, 4.0)),
            "estimate: no published measurement",
        ),
        vec![
            "a top-lit gallery: a laylight over a third of the ceiling between deep coffers, its fabric over an absorbent void the glass wool row; twelve glazed cases and four benches as solids, partitions plasterboard with plinths 0.5 m deep",
        ],
    ))
}
