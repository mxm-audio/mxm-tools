use super::*;

pub fn club_venue() -> Result<Space, Error> {
    let walls = surface(
        "walls",
        &[
            part(m::BLOCK_PAINTED, 0.25, 0.02),
            part(m::GLASS_WOOL, 0.35, 0.05),
            part(m::VELOUR, 0.10, 0.1),
            part(m::WOOD_LINING, 0.30, 0.0),
        ],
    )?;
    let floor = surface(
        "floor",
        &[
            part(m::CONCRETE_FLOOR, 0.85, 0.0),
            relief(m::HARD, 0.15, 1.0),
        ],
    )?;
    let ceiling = surface(
        "ceiling",
        &[part(m::GLASS_WOOL, 0.45, 0.0), relief(m::HARD, 0.55, 1.0)],
    )?;
    Ok(Space {
        room: Room::shoebox(v(32.3, 11.6, 8.0), box_faces(&walls, floor, ceiling))?,
        air: Air::standard(),
        sources: sources(v(2.5, 5.8, 1.8), v(3.0, 2.0, 2.5), v(3.0, 9.6, 2.5)),
        positions: positions(v(10.0, 5.2, 1.6), v(26.0, 6.8, 1.6)),
        non_diffuse: false,
        seam_hz: None,
        max_time_s: 3.0,
        occupancy: "unoccupied standing floor",
        reference: gated(
            Decay::T30,
            Target::Mean {
                first: 1,
                last: 4,
                range: (0.6, 1.6),
            },
            "Adelman-Larsen 2010 Table I: 20 rock venues, T30 mean over 250 Hz–2 kHz",
        ),
        notes: vec![
            "the research notes' rectangular hall, 32.3 × 11.6 × 8 m",
            "the left and right sources are the PA stacks, the centre the band",
            "the bar, stage and ducts by characteristic depth",
        ],
    })
}

pub fn indoor_arena() -> Result<Space, Error> {
    let floor = surface("arena floor", &[part(m::CONCRETE_FLOOR, 1.0, 0.0)])?;
    let tiers = surface(
        "seating tiers",
        &[
            part(m::SEATS, 0.85, 0.45),
            part(m::CONCRETE_FLOOR, 0.15, 0.0),
        ],
    )?;
    let walls = surface(
        "concourse walls",
        &[
            relief(m::HARD, 0.4, 1.0),
            part(m::GLASS_WOOL, 0.4, 0.0),
            part(m::GLASS, 0.2, 0.0),
        ],
    )?;
    let roof = surface(
        "roof",
        &[
            part(m::GLASS_WOOL, 0.4, 0.0),
            part(m::WOOD_LINING, 0.5, 0.0),
            relief(m::HARD, 0.1, 1.5),
        ],
    )?;
    // A bowl: an inner floor, four raked tiers rising 14 m to an outer rim, walls to a roof at
    // 24 m. Tiers on two sides only left the ends facing each other over open floor, and 125 Hz
    // decayed for over 9 s.
    const RIM: f64 = 14.0;
    const ROOF: f64 = 24.0;
    let outer = [(0.0, 0.0), (90.0, 0.0), (90.0, 110.0), (0.0, 110.0)];
    let inner = [(25.0, 30.0), (65.0, 30.0), (65.0, 80.0), (25.0, 80.0)];
    let mut faces: Vec<(Vec<Vec3>, usize)> =
        vec![(inner.iter().map(|&(x, y)| v(x, y, 0.0)).collect(), 0)];
    for i in 0..4 {
        let j = (i + 1) % 4;
        faces.push((
            vec![
                v(inner[i].0, inner[i].1, 0.0),
                v(outer[i].0, outer[i].1, RIM),
                v(outer[j].0, outer[j].1, RIM),
                v(inner[j].0, inner[j].1, 0.0),
            ],
            1,
        ));
        faces.push((
            vec![
                v(outer[i].0, outer[i].1, RIM),
                v(outer[i].0, outer[i].1, ROOF),
                v(outer[j].0, outer[j].1, ROOF),
                v(outer[j].0, outer[j].1, RIM),
            ],
            2,
        ));
    }
    faces.push((outer.iter().rev().map(|&(x, y)| v(x, y, ROOF)).collect(), 3));
    let room = Room::new(faces, vec![floor, tiers, walls, roof])?;
    Ok(Space {
        room,
        air: Air::standard(),
        sources: sources(v(45.0, 34.0, 3.0), v(52.0, 36.0, 12.0), v(38.0, 36.0, 12.0)),
        positions: positions(v(46.0, 55.0, 1.6), v(60.0, 98.0, 10.0)),
        non_diffuse: false,
        seam_hz: None,
        max_time_s: 6.0,
        occupancy: "unoccupied seats",
        reference: reported(
            Decay::T30,
            Target::Mean {
                first: 0,
                last: 4,
                range: (1.3, 2.7),
            },
            "Eşmebaşı Table 1 (secondary): treated arenas, mean 125 Hz–2 kHz",
        ),
        notes: vec![
            "a bowl: raked seating on all four sides of an open floor, a stage at one end",
            "the left and right sources are hung arrays; the far receiver sits on the end tier",
        ],
    })
}

/// Adelman-Larsen 2010 Table I, the club class, for a venue outside its 655–6,500 m³.
fn rock_venues(basis: &'static str) -> Reference {
    reported(
        Decay::T30,
        Target::Mean {
            first: 1,
            last: 4,
            range: (0.6, 1.6),
        },
        basis,
    )
}

pub fn jazz_club() -> Result<Space, Error> {
    let size = v(14.0, 9.0, 3.2);
    let walls = surface(
        "walls",
        &[
            part(m::BRICK, 0.55, 0.05),
            part(m::WOOD_LINING, 0.20, 0.1),
            part(m::VELOUR, 0.10, 0.1),
            part(m::GLASS, 0.05, 0.0),
            part(m::HARD, 0.10, 0.6),
        ],
    )?;
    let floor = surface("floor", &[part(m::WOOD_FLOOR, 1.0, 0.0)])?;
    let ceiling = surface(
        "ceiling",
        &[
            part(m::PLASTERBOARD, 0.6, 0.0),
            part(m::GLASS_WOOL, 0.4, 0.0),
        ],
    )?;
    let bar = surface("bar", &[part(m::WOOD_LINING, 1.0, 0.1)])?;
    let table = surface(
        "tables and chairs",
        &[
            part(m::WOODEN_CHAIRS, 0.25, 0.75),
            part(m::WOOD_LINING, 0.75, 0.0),
        ],
    )?;
    let riser = surface("stage riser", &[part(m::WOOD_FLOOR, 1.0, 0.0)])?;
    let mut solids = vec![
        block((0.3, 2.0), (3.0, 6.0), (0.1, 0.45), &riser),
        block((4.0, 12.0), (0.2, 0.9), (0.1, 1.1), &bar),
    ];
    solids.extend(row_of_blocks(
        6,
        Axis::X,
        (4.5, 12.5),
        0.8,
        (6.0, 7.0),
        (0.1, 0.8),
        &table,
    ));
    Ok(space(
        boxed(size, &walls, floor, ceiling)?.with_solids(&solids)?,
        layout(size),
        2.5,
        "unoccupied tables and chairs",
        rock_venues(
            "Adelman-Larsen 2010 Table I: 20 rock venues of 655–6,500 m³, T30 mean 250 Hz–2 kHz, a larger class",
        ),
        vec![
            "a stage riser, a bar and six tables as solids; the floor is boards alone",
            "a low brick room; the bar is the hard row 0.6 m deep, tables and chairs the wooden chairs row",
        ],
    ))
}

pub fn brick_pub() -> Result<Space, Error> {
    let size = v(12.0, 8.0, 3.0);
    let walls = surface(
        "walls",
        &[
            part(m::BRICK, 0.40, 0.0),
            part(m::WOOD_LINING, 0.35, 0.2),
            part(m::GLASS, 0.15, 0.0),
            part(m::HARD, 0.10, 0.6),
        ],
    )?;
    let floor = surface("floor", &[part(m::WOOD_FLOOR, 1.0, 0.0)])?;
    let ceiling = surface(
        "ceiling",
        &[part(m::PLASTER, 0.7, 0.2), part(m::WOOD_LINING, 0.3, 0.2)],
    )?;
    let bar = surface("bar and back fitting", &[part(m::WOOD_LINING, 1.0, 0.1)])?;
    let banquette = surface(
        "banquettes",
        &[
            part(m::MEDIUM_SEATS, 0.25, 0.6),
            part(m::WOOD_LINING, 0.75, 0.0),
        ],
    )?;
    let table = surface(
        "tables and stools",
        &[
            part(m::WOODEN_CHAIRS, 0.25, 0.75),
            part(m::WOOD_LINING, 0.75, 0.0),
        ],
    )?;
    let mut solids = vec![
        block((2.6, 9.4), (0.15, 1.05), (0.1, 1.1), &bar),
        block((2.6, 9.4), (6.85, 7.85), (0.1, 0.9), &banquette),
        block((0.2, 1.2), (2.0, 6.0), (0.1, 0.9), &banquette),
    ];
    solids.extend(row_of_blocks(
        4,
        Axis::X,
        (2.8, 9.4),
        1.4,
        (4.4, 6.1),
        (0.1, 0.75),
        &table,
    ));
    Ok(space(
        boxed(size, &walls, floor, ceiling)?.with_solids(&solids)?,
        layout(size),
        2.5,
        "unoccupied",
        reported(
            Decay::T30,
            mid((0.6, 1.0)),
            "estimate: no published measurement",
        ),
        vec![
            "a bar, banquettes and four tables as solids, each a quarter of its row to three quarters frame; the floor is boards alone",
            "panelled booths are medium upholstered seats over a quarter of the floor; beams by depth (0.2 m)",
        ],
    ))
}

pub fn ballroom() -> Result<Space, Error> {
    let size = v(36.0, 20.0, 10.0);
    let walls = surface(
        "walls",
        &[
            relief(m::PLASTER, 0.40, 0.6),
            part(m::PLATE_GLASS, 0.15, 0.0),
            part(m::GLASS, 0.15, 0.0),
            part(m::VELOUR, 0.30, 0.2),
        ],
    )?;
    let floor = surface(
        "floor",
        &[
            part(m::PARQUET, 0.60, 0.0),
            part(m::MEDIUM_SEATS, 0.25, 0.6),
            part(m::CARPET, 0.15, 0.01),
        ],
    )?;
    let ceiling = surface("ceiling", &[relief(m::PLASTER, 1.0, 1.0)])?;
    let pilaster = surface("pilasters", &[part(m::PLASTER, 1.0, 0.0)])?;
    let rib = surface("ceiling ribs", &[part(m::PLASTER, 1.0, 0.0)])?;
    let solids = pilasters_and_ribs(
        size,
        Axis::X,
        (1.5, 34.5),
        14,
        (0.7, 0.6, (0.35, 6.5)),
        (10, (9.0, 9.45), 0.5),
        &pilaster,
        &rib,
    );
    Ok(space(
        boxed_with(size, &walls, floor, ceiling, &solids)?,
        layout(size),
        5.0,
        "an empty dance floor, banquettes along the walls",
        reported(
            Decay::T30,
            mid((1.4, 2.2)),
            "estimate: no published measurement",
        ),
        vec![
            "mirrors are the plate glass row, drapes over 30 % of the walls; twenty-eight pilasters and ten ceiling ribs as solids, coffers and chandeliers still by depth; a carpet border",
        ],
    ))
}

pub fn large_music_venue() -> Result<Space, Error> {
    let size = v(45.0, 30.0, 14.0);
    let walls = surface(
        "walls",
        &[
            part(m::GLASS_WOOL, 0.35, 0.1),
            part(m::BASS_TRAP, 0.10, 0.3),
            relief(m::BLOCK_PAINTED, 0.35, 0.5),
            relief(m::HARD, 0.10, 1.5),
            part(m::VELOUR, 0.10, 0.2),
        ],
    )?;
    let floor = surface(
        "floor",
        &[part(m::CONCRETE_FLOOR, 0.9, 0.0), relief(m::HARD, 0.1, 1.0)],
    )?;
    let ceiling = surface(
        "roof and rig",
        &[
            part(m::GLASS_WOOL, 0.3, 0.0),
            part(m::BASS_TRAP, 0.1, 0.3),
            relief(m::HARD, 0.6, 1.5),
        ],
    )?;
    Ok(space(
        boxed(size, &walls, floor, ceiling)?,
        (
            sources(v(3.0, 15.0, 2.0), v(3.5, 7.0, 3.0), v(3.5, 23.0, 3.0)),
            positions(v(14.0, 13.5, 1.6), v(38.0, 17.0, 1.6)),
        ),
        6.5,
        "unoccupied standing floor",
        reported(
            Decay::T30,
            Target::Mean {
                first: 0,
                last: 4,
                range: (1.0, 5.0),
            },
            "Adelman-Larsen & Dammerud 2011 Fig. 2: venues of 7,000–600,000 m³, T30 mean 125 Hz–2 kHz, read off",
        ),
        vec![
            "a black box: absorbent walls and roof with bass traps (the plywood-over-mineral-fibre row), bars and stairs 1.0–1.5 m deep",
            "the left and right sources are the PA stacks, the centre the band",
        ],
    ))
}

pub fn sports_hall() -> Result<Space, Error> {
    let size = v(44.0, 26.0, 9.0);
    let walls = surface(
        "walls",
        &[
            part(m::BLOCK_PAINTED, 0.60, 0.3),
            part(m::WOOD_LINING, 0.15, 0.3),
            part(m::GLASS, 0.10, 0.0),
            part(m::PERFORATED_WOOD, 0.15, 0.05),
        ],
    )?;
    let floor = surface("sprung floor", &[part(m::WOOD_FLOOR, 1.0, 0.0)])?;
    let ceiling = surface(
        "roof",
        &[
            part(m::PERFORATED_METAL, 0.35, 1.2),
            part(m::HARD, 0.55, 1.2),
            part(m::GLASS, 0.10, 0.0),
        ],
    )?;
    let pilaster = surface("pilasters", &[part(m::BLOCK_PAINTED, 1.0, 0.0)])?;
    let rib = surface("ceiling ribs", &[part(m::HARD, 1.0, 0.0)])?;
    let solids = pilasters_and_ribs(
        size,
        Axis::X,
        (2.0, 42.0),
        14,
        (0.6, 0.4, (0.35, 6.0)),
        (10, (7.9, 8.35), 0.5),
        &pilaster,
        &rib,
    );
    Ok(space(
        boxed_with(size, &walls, floor, ceiling, &solids)?,
        layout(size),
        5.0,
        "empty",
        reported(
            Decay::T30,
            mid((1.6, 2.5)),
            "estimate: between a sports hall's 2.0 s design target (survey [40]) and an untreated arena",
        ),
        vec![
            "a perforated metal deck over a third of the roof; twenty-eight pilasters and ten roof trusses as solids; wall bars and impact panels by depth",
        ],
    ))
}

pub fn velodrome() -> Result<Space, Error> {
    let plan: Vec<(f64, f64)> = (0..16)
        .map(|k| {
            let a = 2.0 * PI * k as f64 / 16.0;
            (80.0 + 80.0 * a.cos(), 45.0 + 45.0 * a.sin())
        })
        .collect();
    let floor = surface(
        "track and infield",
        &[
            part(m::WOOD_FLOOR, 0.35, 0.0),
            part(m::CONCRETE_FLOOR, 0.40, 0.0),
            part(m::SEATS, 0.25, 0.45),
        ],
    )?;
    let roof = surface(
        "roof",
        &[
            relief(m::HARD, 0.60, 2.5),
            relief(m::PERFORATED_METAL, 0.25, 2.5),
            part(m::GLASS, 0.15, 0.0),
        ],
    )?;
    let walls = surface(
        "stands and walls",
        &[
            part(m::SEATS, 0.4, 0.45),
            relief(m::HARD, 0.4, 1.0),
            part(m::GLASS, 0.2, 0.0),
        ],
    )?;
    Ok(space(
        Room::extruded(&plan, 30.0, [floor, roof, walls])?,
        (
            sources(v(40.0, 45.0, 3.0), v(40.0, 38.0, 3.0), v(40.0, 52.0, 3.0)),
            positions(v(70.0, 43.0, 1.6), v(135.0, 50.0, 8.0)),
        ),
        12.0,
        "unoccupied",
        reported(
            Decay::T30,
            mid((4.0, 7.0)),
            "estimate: survey row 15's 7 s empty velodrome, a value research notes §21 could not confirm",
        ),
        vec![
            "an oval of sixteen walls 160 × 90 m under a roof 30 m up; the banked track is not modelled",
            "the render ends at 10 s with a 0.5 s fade; metrics come from the uncapped render",
        ],
    ))
}

pub fn village_hall() -> Result<Space, Error> {
    let size = v(22.0, 11.0, 6.5);
    let walls = surface(
        "walls",
        &[
            part(m::PLASTER, 0.45, 0.1),
            part(m::WOOD_LINING, 0.20, 0.1),
            part(m::GLASS, 0.20, 0.0),
            part(m::CURTAIN, 0.15, 0.2),
        ],
    )?;
    let floor = surface(
        "floor",
        &[
            part(m::WOOD_FLOOR, 0.8, 0.0),
            part(m::WOODEN_CHAIRS, 0.2, 0.75),
        ],
    )?;
    let ceiling = surface(
        "roof",
        &[
            relief(m::WOOD_LINING, 0.60, 0.8),
            relief(m::PLASTERBOARD, 0.15, 0.8),
            part(m::MINERAL_BOARD, 0.25, 0.0),
        ],
    )?;
    let pilaster = surface("pilasters", &[part(m::WOOD_LINING, 1.0, 0.0)])?;
    let rib = surface("ceiling ribs", &[part(m::WOOD_LINING, 1.0, 0.0)])?;
    let solids = pilasters_and_ribs(
        size,
        Axis::X,
        (1.2, 20.8),
        8,
        (0.5, 0.4, (0.35, 4.2)),
        (6, (5.7, 6.05), 0.45),
        &pilaster,
        &rib,
    );
    Ok(space(
        boxed_with(size, &walls, floor, ceiling, &solids)?,
        layout(size),
        3.5,
        "stacked chairs",
        reported(
            Decay::T30,
            mid((1.2, 1.8)),
            "estimate: no published measurement",
        ),
        vec![
            "a timber roof with trusses 0.8 m deep and ceiling tiles between; sixteen wall posts and six roof trusses as solids; curtains at the windows and the stage",
        ],
    ))
}

pub fn basement_club() -> Result<Space, Error> {
    let size = v(18.0, 10.0, 3.0);
    let walls = surface(
        "walls",
        &[
            part(m::BLOCK_PAINTED, 0.5, 0.0),
            part(m::GLASS_WOOL, 0.2, 0.05),
            part(m::BRICK, 0.2, 0.0),
            part(m::HARD, 0.1, 0.6),
        ],
    )?;
    let floor = surface("floor", &[part(m::CONCRETE_FLOOR, 1.0, 0.0)])?;
    let ceiling = surface(
        "ceiling",
        &[part(m::HARD, 0.7, 0.3), part(m::GLASS_WOOL, 0.3, 0.0)],
    )?;
    let bar = surface("bar", &[part(m::WOOD_LINING, 1.0, 0.1)])?;
    let stack = surface("PA stacks", &[part(m::HARD, 1.0, 0.2)])?;
    let solids = [
        block((6.0, 14.0), (0.2, 1.0), (0.1, 1.1), &bar),
        block((1.0, 1.8), (2.0, 3.0), (0.2, 2.0), &stack),
        block((1.0, 1.8), (7.0, 8.0), (0.2, 2.0), &stack),
    ];
    Ok(space(
        boxed(size, &walls, floor, ceiling)?.with_solids(&solids)?,
        layout(size),
        3.0,
        "unoccupied standing floor",
        rock_venues(
            "Adelman-Larsen 2010 Table I: 20 rock venues of 655–6,500 m³, T30 mean 250 Hz–2 kHz, a larger class",
        ),
        vec![
            "a bar and two PA stacks as solids; a low brick cellar",
            "a low concrete basement; ducts 0.3 m and the bar 0.6 m deep, absorbent panels on a fifth of the walls",
        ],
    ))
}

pub fn swimming_hall() -> Result<Space, Error> {
    let size = v(50.0, 25.0, 10.0);
    let walls = surface(
        "walls",
        &[
            part(m::TILE, 0.30, 0.0),
            part(m::PERFORATED_WOOD, 0.15, 0.1),
            part(m::GLASS, 0.40, 0.3),
            part(m::BLOCK_PAINTED, 0.15, 0.3),
        ],
    )?;
    let floor = surface(
        "pool and surround",
        &[
            part(m::WATER, 0.5, 0.0),
            part(m::TILE, 0.45, 0.0),
            part(m::HARD, 0.05, 0.5),
        ],
    )?;
    let ceiling = surface(
        "roof",
        &[
            part(m::PERFORATED_METAL, 0.45, 1.2),
            part(m::HARD, 0.45, 1.2),
            part(m::GLASS, 0.10, 0.0),
        ],
    )?;
    let pilaster = surface("pilasters", &[part(m::TILE, 1.0, 0.0)])?;
    let rib = surface("ceiling ribs", &[part(m::HARD, 1.0, 0.0)])?;
    let solids = pilasters_and_ribs(
        size,
        Axis::X,
        (2.0, 48.0),
        14,
        (0.6, 0.5, (0.35, 6.5)),
        (10, (8.8, 9.25), 0.5),
        &pilaster,
        &rib,
    );
    Ok(space(
        boxed_with(size, &walls, floor, ceiling, &solids)?,
        layout(size),
        7.0,
        "empty, still water",
        reported(
            Decay::T30,
            mid((2.0, 3.5)),
            "estimate: above a pool hall's 2.0 s design target (survey [40]), with no absorbers on the walls",
        ),
        vec![
            "a pool over half the floor; a perforated deck over 45 % of the roof; twenty-eight wall piers and ten roof trusses as solids; perforated wood panels on 15 % of the walls, benches and starting blocks by depth",
        ],
    ))
}

pub fn ice_rink() -> Result<Space, Error> {
    let size = v(60.0, 30.0, 11.0);
    let walls = surface(
        "walls",
        &[
            part(m::BLOCK_PAINTED, 0.6, 0.3),
            part(m::GLASS, 0.2, 0.0),
            part(m::HARD, 0.2, 0.5),
        ],
    )?;
    let floor = surface(
        "ice and boards",
        &[
            part(m::HARD, 0.75, 0.0),
            part(m::WOOD_LINING, 0.10, 1.1),
            part(m::SEATS, 0.15, 0.45),
        ],
    )?;
    let ceiling = surface(
        "roof",
        &[
            part(m::HARD, 0.75, 2.0),
            part(m::PERFORATED_METAL, 0.25, 2.0),
        ],
    )?;
    let pilaster = surface("pilasters", &[part(m::BLOCK_PAINTED, 1.0, 0.0)])?;
    let rib = surface("ceiling ribs", &[part(m::HARD, 1.0, 0.0)])?;
    let solids = pilasters_and_ribs(
        size,
        Axis::X,
        (2.0, 58.0),
        16,
        (0.6, 0.5, (0.35, 7.0)),
        (10, (9.8, 10.3), 0.5),
        &pilaster,
        &rib,
    );
    Ok(space(
        boxed_with(size, &walls, floor, ceiling, &solids)?,
        layout(size),
        8.0,
        "empty rink, a stand along one side",
        reported(
            Decay::T30,
            Target::Mean {
                first: 1,
                last: 4,
                range: (2.2, 2.2),
            },
            "Eşmebaşı Table 1 (secondary): one treated ice arena of 53,000 m³, 2.2 s over 250 Hz–2 kHz (untreated, 6.2 s over 125 Hz–4 kHz)",
        ),
        vec![
            "no row for ice: the hard row stands for it; dasher boards are wood lining 1.1 m deep; thirty-two wall piers and ten roof trusses as solids",
        ],
    ))
}
