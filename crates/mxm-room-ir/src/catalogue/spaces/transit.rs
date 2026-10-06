use super::*;

pub fn car_park() -> Result<Space, Error> {
    let walls = surface("walls", &[part(m::BLOCK_PAINTED, 1.0, 0.0)])?;
    let floor = surface("floor", &[part(m::CONCRETE_FLOOR, 1.0, 0.0)])?;
    let slab = surface("slab and beams", &[relief(m::HARD, 1.0, 0.4)])?;
    let cars = surface(
        "parked cars",
        &[relief(m::GLASS, 0.35, 0.2), relief(m::HARD, 0.65, 0.3)],
    )?;
    let prisms: Vec<Prism> = [(2.5, 7.0), (11.0, 15.5), (20.5, 25.0), (29.0, 33.5)]
        .into_iter()
        .map(|y| Prism {
            section: vec![(6.0, 0.0), (51.0, 0.0), (51.0, 1.45), (6.0, 1.45)],
            y,
            material: cars.clone(),
        })
        .collect();
    let room = Room::box_with_prisms(
        v(0.0, 0.0, 0.0),
        v(60.0, 36.0, 2.6),
        [
            walls.clone(),
            walls.clone(),
            walls.clone(),
            walls,
            floor,
            slab,
        ],
        &prisms,
    )?;
    Ok(Space {
        room,
        air: Air::standard(),
        sources: sources(v(8.0, 9.0, 1.6), v(8.0, 8.0, 1.6), v(8.0, 10.0, 1.6)),
        positions: positions(v(16.0, 9.3, 1.6), v(50.0, 18.0, 1.6)),
        non_diffuse: true,
        seam_hz: None,
        max_time_s: 5.0,
        occupancy: "four rows of parked cars",
        reference: reported(
            Decay::T30,
            mid((3.0, 3.0)),
            "estimate (research notes §18): a course handout's ≈ 3 s",
        ),
        notes: vec![
            "each row of parked cars is one block 45 m long; columns are left out",
            "a flat, wide room: its field is two-dimensional, so the scene is non-diffuse",
        ],
    })
}

pub fn rail_tunnel() -> Result<Space, Error> {
    const LENGTH: f64 = 400.0;
    const HALF_WIDTH: f64 = 4.2;
    const SPRING: f64 = 2.2;
    let portals = Material::anechoic();
    let floor = surface(
        "slab track",
        &[
            part(m::CONCRETE_FLOOR, 0.9, 0.0),
            part(m::BALLAST, 0.1, 0.1),
        ],
    )?;
    let lining = surface("concrete lining", &[part(m::HARD, 1.0, 0.05)])?;
    // Cross-section in (y, z), counter-clockwise: the floor, a vertical side, a semicircular arch
    // of eight facets, the other side.
    let mut section = vec![(-HALF_WIDTH, 0.0), (HALF_WIDTH, 0.0), (HALF_WIDTH, SPRING)];
    for i in 1..8 {
        let theta = PI * i as f64 / 8.0;
        section.push((HALF_WIDTH * theta.cos(), SPRING + HALF_WIDTH * theta.sin()));
    }
    section.push((-HALF_WIDTH, SPRING));
    let n = section.len();
    let mut faces: Vec<(Vec<Vec3>, usize)> = (0..n)
        .map(|i| {
            let (a, b) = (section[i], section[(i + 1) % n]);
            (
                vec![
                    v(0.0, a.0, a.1),
                    v(LENGTH, a.0, a.1),
                    v(LENGTH, b.0, b.1),
                    v(0.0, b.0, b.1),
                ],
                if i == 0 { 1 } else { 2 },
            )
        })
        .collect();
    faces.push((section.iter().map(|&(y, z)| v(0.0, y, z)).collect(), 0));
    faces.push((
        section
            .iter()
            .rev()
            .map(|&(y, z)| v(LENGTH, y, z))
            .collect(),
        0,
    ));
    let room = Room::new(faces, vec![portals, floor, lining])?;
    Ok(Space {
        room,
        air: Air::standard(),
        sources: sources(v(200.0, 0.0, 1.6), v(200.0, -1.5, 1.6), v(200.0, 1.5, 1.6)),
        positions: positions(v(215.0, 0.8, 1.6), v(300.0, -1.0, 1.6)),
        non_diffuse: true,
        seam_hz: None,
        max_time_s: 12.0,
        occupancy: "no train",
        reference: reported(
            Decay::T30,
            octaves([
                (8.9, 10.2),
                (6.5, 7.6),
                (5.6, 5.9),
                (5.5, 5.7),
                (4.1, 4.3),
                (2.6, 2.7),
            ]),
            "Ridley & Spearritt Table 8: one rectangular road tunnel, a proxy",
        ),
        notes: vec![
            "400 m of a longer tunnel: both cut ends absorb, standing for the tunnel beyond",
            "an arch of eight facets over a slab track with ballast in the cable troughs",
            "the render ends at 10 s with a 0.5 s fade; metrics come from the uncapped render",
        ],
    })
}

pub fn station_concourse() -> Result<Space, Error> {
    let walls = surface(
        "walls",
        &[
            part(m::BLOCK_PAINTED, 0.5, 0.5),
            part(m::GLASS, 0.25, 0.0),
            part(m::HARD, 0.10, 1.0),
            part(m::GLASS_WOOL, 0.15, 0.0),
        ],
    )?;
    let floor = surface("floor", &[part(m::CONCRETE_FLOOR, 1.0, 0.0)])?;
    let roof = surface(
        "roof",
        &[
            part(m::HARD, 0.3, 1.5),
            part(m::GLASS, 0.2, 0.0),
            part(m::GLASS_WOOL, 0.5, 0.0),
        ],
    )?;
    let kiosk = surface(
        "kiosks",
        &[
            part(m::PLATE_GLASS, 0.5, 0.0),
            part(m::WOOD_LINING, 0.5, 0.2),
        ],
    )?;
    let bench = surface(
        "benches",
        &[part(m::WOODEN_CHAIRS, 0.25, 0.0), part(m::HARD, 0.75, 0.0)],
    )?;
    let mut solids = row_of_blocks(
        3,
        Axis::X,
        (20.0, 64.0),
        6.0,
        (30.0, 34.0),
        (0.3, 3.2),
        &kiosk,
    );
    solids.extend(row_of_blocks(
        4,
        Axis::X,
        (18.0, 62.0),
        2.4,
        (12.0, 13.0),
        (0.2, 0.6),
        &bench,
    ));
    Ok(Space {
        room: Room::shoebox(v(80.0, 40.0, 12.0), box_faces(&walls, floor, roof))?
            .with_solids(&solids)?,
        air: Air::standard(),
        sources: sources(v(10.0, 20.0, 2.0), v(10.0, 17.0, 2.0), v(10.0, 23.0, 2.0)),
        positions: positions(v(25.0, 18.5, 1.6), v(65.0, 24.0, 1.6)),
        non_diffuse: false,
        seam_hz: None,
        max_time_s: 6.0,
        occupancy: "empty",
        reference: reported(
            Decay::T30,
            mid((1.9, 3.0)),
            "AAS2015 paper 94 Table 5: one concourse, five positions, mid band",
        ),
        notes: vec![
            "three kiosks and four benches as solids; a concourse under a glazed roof",
            "columns, stairs, kiosks and roof trusses by characteristic depth (0.5–1.5 m); skylights over a fifth of the roof",
        ],
    })
}

pub fn pedestrian_underpass() -> Result<Space, Error> {
    let floor = surface("floor", &[part(m::CONCRETE_FLOOR, 1.0, 0.0)])?;
    let walls = surface(
        "walls",
        &[part(m::TILE, 0.7, 0.0), part(m::BLOCK_PAINTED, 0.3, 0.0)],
    )?;
    let ceiling = surface(
        "soffit",
        &[part(m::HARD, 0.7, 0.1), part(m::PERFORATED_METAL, 0.3, 0.1)],
    )?;
    let section = [
        ((0.0, 0.0), 1),
        ((6.0, 0.0), 2),
        ((6.0, 3.0), 3),
        ((0.0, 3.0), 2),
    ];
    Ok(Space {
        non_diffuse: true,
        ..space(
            swept(
                Axis::X,
                &section,
                36.0,
                0,
                vec![Material::anechoic(), floor, walls, ceiling],
            )?,
            (
                sources(v(8.0, 3.0, 1.6), v(8.0, 2.0, 1.6), v(8.0, 4.0, 1.6)),
                positions(v(14.0, 2.7, 1.6), v(27.0, 3.6, 1.6)),
            ),
            3.5,
            "empty",
            reported(
                Decay::T30,
                mid((1.0, 2.0)),
                "estimate: no published measurement",
            ),
            vec![
                "36 m of tiled underpass under a panel ceiling (30 % perforated metal), both ends open: the end faces absorb, standing for the street beyond; below the seam their fitted boundary reflects about 5 %",
                "a long, narrow space keeps its flutter discrete, so the scene is non-diffuse",
            ],
        )
    })
}

pub fn metro_platform() -> Result<Space, Error> {
    let ballast = surface(
        "track bed",
        &[
            part(m::BALLAST, 0.5, 0.3),
            part(m::CONCRETE_FLOOR, 0.5, 0.0),
        ],
    )?;
    let edge = surface("platform edge", &[part(m::HARD, 1.0, 0.0)])?;
    let platform = surface(
        "platform",
        &[part(m::TILE, 0.8, 0.0), relief(m::HARD, 0.2, 0.5)],
    )?;
    let wall = surface(
        "platform wall",
        &[
            part(m::TILE, 0.5, 0.0),
            part(m::BLOCK_PAINTED, 0.3, 0.0),
            part(m::GLASS, 0.2, 0.0),
        ],
    )?;
    let ceiling = surface(
        "ceiling",
        &[
            relief(m::HARD, 0.75, 0.5),
            relief(m::PERFORATED_METAL, 0.25, 0.5),
        ],
    )?;
    let track_wall = surface("track wall", &[part(m::BLOCK_PAINTED, 1.0, 0.0)])?;
    // Cross-section in (y, z): the track bed, the platform edge, the platform 1.1 m up, its wall,
    // the ceiling, the track wall.
    let section = [
        ((0.0, 0.0), 1),
        ((5.0, 0.0), 2),
        ((5.0, 1.1), 3),
        ((16.0, 1.1), 4),
        ((16.0, 6.5), 5),
        ((0.0, 6.5), 6),
    ];
    Ok(Space {
        non_diffuse: true,
        ..space(
            swept(
                Axis::X,
                &section,
                120.0,
                0,
                vec![
                    Material::anechoic(),
                    ballast,
                    edge,
                    platform,
                    wall,
                    ceiling,
                    track_wall,
                ],
            )?,
            (
                sources(v(20.0, 10.5, 2.7), v(20.0, 8.0, 2.7), v(20.0, 13.0, 2.7)),
                positions(v(34.0, 10.0, 2.7), v(90.0, 11.3, 2.7)),
            ),
            5.0,
            "no train",
            reported(
                Decay::T30,
                octaves([
                    (1.73, 1.73),
                    (2.44, 2.44),
                    (2.05, 2.05),
                    (1.71, 1.71),
                    (1.47, 1.47),
                    (1.11, 1.11),
                ]),
                "Hládek et al. Table I: one underground platform",
            ),
            vec![
                "a 120 m platform beside one track; the ends open into running tunnels and absorb",
                "benches and signs by depth; a perforated metal ceiling over a quarter",
                "a long, low space keeps its specular structure discrete, so the scene is non-diffuse",
            ],
        )
    })
}

pub fn road_tunnel() -> Result<Space, Error> {
    let road = surface(
        "road",
        &[part(m::CONCRETE_FLOOR, 0.9, 0.0), relief(m::HARD, 0.1, 0.2)],
    )?;
    let walls = surface(
        "walls",
        &[
            part(m::HARD, 0.815, 0.05),
            part(m::BLOCK_PAINTED, 0.15, 0.0),
            part(m::PERFORATED_METAL, 0.035, 0.05),
        ],
    )?;
    let ceiling = surface(
        "soffit and fans",
        &[part(m::HARD, 0.9, 0.05), relief(m::HARD, 0.1, 1.0)],
    )?;
    let section = [
        ((0.0, 0.0), 1),
        ((10.5, 0.0), 2),
        ((10.5, 6.2), 3),
        ((0.0, 6.2), 2),
    ];
    Ok(Space {
        non_diffuse: true,
        ..space(
            swept(
                Axis::X,
                &section,
                400.0,
                0,
                vec![Material::anechoic(), road, walls, ceiling],
            )?,
            (
                sources(
                    v(200.0, 5.25, 1.6),
                    v(200.0, 3.75, 1.6),
                    v(200.0, 6.75, 1.6),
                ),
                positions(v(215.0, 4.9, 1.6), v(300.0, 6.0, 1.6)),
            ),
            12.0,
            "no traffic",
            reported(
                Decay::T30,
                octaves([
                    (8.9, 10.2),
                    (6.5, 7.6),
                    (5.6, 5.9),
                    (5.5, 5.7),
                    (4.1, 4.3),
                    (2.6, 2.7),
                ]),
                "Ridley & Spearritt Table 8: one rectangular road tunnel, four positions",
            ),
            vec![
                "400 m of a longer rectangular tunnel: both cut ends absorb, standing for the tunnel beyond",
                "no row for asphalt: the concrete floor row stands for it; jet fans by depth (1.0 m); absorbent panels at the lay-bys over 3.5 % of the walls",
                "the render ends at 10 s with a 0.5 s fade; metrics come from the uncapped render",
            ],
        )
    })
}

pub fn airport_terminal() -> Result<Space, Error> {
    let size = v(120.0, 60.0, 18.0);
    let walls = surface(
        "curtain walls and cores",
        &[
            part(m::PLATE_GLASS, 0.55, 1.0),
            part(m::HARD, 0.30, 1.0),
            part(m::GLASS_WOOL, 0.15, 0.1),
        ],
    )?;
    let floor = surface(
        "floor",
        &[part(m::TILE, 0.75, 0.0), part(m::CARPET, 0.25, 0.01)],
    )?;
    let ceiling = surface(
        "ceiling",
        &[
            part(m::PERFORATED_METAL, 0.6, 1.5),
            part(m::PLATE_GLASS, 0.2, 0.0),
            part(m::HARD, 0.2, 1.5),
        ],
    )?;
    let seating = surface(
        "gate seating",
        &[part(m::MEDIUM_SEATS, 0.25, 0.6), part(m::HARD, 0.75, 0.0)],
    )?;
    let kiosk = surface(
        "kiosks and desks",
        &[
            part(m::PLATE_GLASS, 0.5, 0.0),
            part(m::WOOD_LINING, 0.5, 0.2),
        ],
    )?;
    let mut solids = row_of_blocks(
        6,
        Axis::X,
        (30.0, 100.0),
        6.0,
        (40.0, 42.0),
        (0.2, 0.9),
        &seating,
    );
    solids.extend(row_of_blocks(
        3,
        Axis::X,
        (34.0, 96.0),
        8.0,
        (10.0, 16.0),
        (0.3, 3.5),
        &kiosk,
    ));
    Ok(space(
        boxed(size, &walls, floor, ceiling)?.with_solids(&solids)?,
        layout(size),
        9.0,
        "empty hall, rows of seats",
        reported(
            Decay::T30,
            mid((2.0, 3.5)),
            "estimate: below the enclosed shopping halls' 2.7–5.1 s (Kanev 2021), with an absorbing ceiling",
        ),
        vec![
            "six runs of gate seating and three kiosks as solids; seating no longer stands in the floor",
            "stone floor as the glazed tile row, carpeted gate lounges; curtain-wall mullions and fins 1.0 m deep; desks and kiosks 1.5 m deep; a perforated metal ceiling over 60 % of the roof",
        ],
    ))
}

pub fn glass_atrium() -> Result<Space, Error> {
    let size = v(32.0, 30.0, 26.0);
    let walls = surface(
        "walls",
        &[
            part(m::PLATE_GLASS, 0.40, 0.0),
            relief(m::PLASTER, 0.20, 0.5),
            relief(m::HARD, 0.15, 1.5),
            part(m::CARPET, 0.25, 1.5),
        ],
    )?;
    let floor = surface(
        "floor",
        &[
            part(m::TILE, 0.65, 0.0),
            relief(m::HARD, 0.10, 0.8),
            part(m::MEDIUM_SEATS, 0.10, 0.6),
            part(m::BALLAST, 0.15, 0.3),
        ],
    )?;
    let ceiling = surface(
        "glass roof",
        &[
            relief(m::PLATE_GLASS, 0.6, 0.5),
            relief(m::HARD, 0.2, 0.5),
            relief(m::PERFORATED_METAL, 0.2, 0.5),
        ],
    )?;
    Ok(space(
        boxed(size, &walls, floor, ceiling)?,
        layout(size),
        10.0,
        "café seating, no people",
        reported(
            Decay::T30,
            mid((3.0, 5.0)),
            "estimate: no published measurement",
        ),
        vec![
            "glass walls and roof, sun louvres perforated metal; carpeted galleries open behind balcony fronts (1.5 m deep); gravel planting beds are the ballast row",
        ],
    ))
}

pub fn parking_deck() -> Result<Space, Error> {
    let floor = surface(
        "deck and cars",
        &[
            part(m::CONCRETE_FLOOR, 0.7, 0.0),
            relief(m::HARD, 0.2, 1.4),
            relief(m::GLASS, 0.1, 1.4),
        ],
    )?;
    let parapet = surface("parapets", &[part(m::HARD, 1.0, 0.0)])?;
    let soffit = surface("slab and beams", &[relief(m::HARD, 1.0, 0.6)])?;
    // Cross-section in (y, z): the deck, a parapet to 1.1 m and the opening above it on each side,
    // the slab over.
    let section = [
        ((0.0, 0.0), 1),
        ((40.0, 0.0), 2),
        ((40.0, 1.1), 0),
        ((40.0, 2.9), 3),
        ((0.0, 2.9), 0),
        ((0.0, 1.1), 2),
    ];
    Ok(space(
        swept(
            Axis::X,
            &section,
            64.0,
            0,
            vec![Material::anechoic(), floor, parapet, soffit],
        )?,
        (
            sources(v(8.0, 20.0, 1.6), v(8.0, 17.0, 1.6), v(8.0, 23.0, 1.6)),
            positions(v(20.0, 19.0, 1.6), v(54.0, 21.5, 1.6)),
        ),
        4.0,
        "parked cars",
        reported(
            Decay::T30,
            mid((1.0, 2.5)),
            "estimate: an open-sided deck, shorter than the enclosed car park's ≈ 3 s (research notes §18)",
        ),
        vec![
            "open on every side: above 1.1 m parapets along its length, and at both ends onto the ramps and the next bays; the openings absorb, standing for the air and the deck beyond",
            "parked cars by depth (1.4 m) over 30 % of the deck scatter sound into the open sides, so the scene is diffuse; columns are left out",
        ],
    ))
}
