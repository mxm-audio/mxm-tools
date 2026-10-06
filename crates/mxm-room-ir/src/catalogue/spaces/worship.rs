use super::*;

pub fn small_chapel() -> Result<Space, Error> {
    let nave = [
        surface(
            "nave floor",
            &[
                part(m::WOOD_FLOOR, 0.35, 0.0),
                part(m::PEWS, 0.55, 0.3),
                part(m::CARPET, 0.10, 0.01),
            ],
        )?,
        surface("timber ceiling", &[part(m::WOOD_LINING, 1.0, 0.1)])?,
        surface(
            "nave walls",
            &[
                part(m::PLASTER, 0.57, 0.0),
                part(m::WOOD_LINING, 0.25, 0.0),
                part(m::LEADED_GLAZING, 0.15, 0.0),
                part(m::VELOUR, 0.03, 0.1),
            ],
        )?,
    ];
    let chancel = [
        surface("chancel floor", &[part(m::SANDSTONE, 1.0, 0.0)])?,
        surface("chancel ceiling", &[part(m::PLASTER, 1.0, 0.0)])?,
        surface(
            "chancel walls",
            &[
                part(m::PLASTER, 0.85, 0.0),
                part(m::LEADED_GLAZING, 0.15, 0.0),
            ],
        )?,
    ];
    let [nave_floor, nave_ceiling, nave_walls] = nave;
    let [chancel_floor, chancel_ceiling, chancel_walls] = chancel;
    let room = Room::stepped_regions(&[
        Region {
            plan: vec![
                (0.0, 0.0),
                (13.0, 0.0),
                (13.0, 1.25),
                (13.0, 5.25),
                (13.0, 6.5),
                (0.0, 6.5),
            ],
            materials: [nave_floor, nave_ceiling, nave_walls],
            height: 6.0,
        },
        Region {
            plan: vec![(13.0, 1.25), (16.5, 1.25), (16.5, 5.25), (13.0, 5.25)],
            materials: [chancel_floor, chancel_ceiling, chancel_walls],
            height: 4.5,
        },
    ])?;
    Ok(Space {
        room,
        air: Air::standard(),
        sources: sources(v(11.5, 3.25, 1.6), v(11.5, 4.5, 1.6), v(11.5, 2.0, 1.6)),
        positions: positions(v(7.5, 3.6, 1.2), v(1.8, 2.8, 1.2)),
        non_diffuse: false,
        seam_hz: None,
        max_time_s: 4.0,
        occupancy: "unoccupied",
        reference: reported(
            Decay::T30,
            octaves([
                (1.17, 1.17),
                (1.51, 1.51),
                (1.75, 1.75),
                (1.84, 1.84),
                (1.86, 1.86),
                (1.60, 1.60),
            ]),
            "Desarnaulds Table 1: one unoccupied chapel of 575 m³ (the second row repeats it)",
        ),
        notes: vec![
            "a lower chancel off the nave; wood panelling to a quarter of the wall",
            "wave-solved below its Schroeder frequency: at 570 m³ it lies under the catalogue's 1,000 m³ limit",
        ],
    })
}

pub fn stone_church() -> Result<Space, Error> {
    // The R2 church's geometry and rows (`examples/render_church.rs`), with scattering now from
    // the depth rule rather than flat values.
    let brick = surface(
        "nave walls",
        &[
            part(m::BRICK, 0.85, 0.6),
            part(m::LEADED_GLAZING, 0.15, 0.3),
        ],
    )?;
    let stone = surface("stone", &[part(m::SANDSTONE, 1.0, 0.6)])?;
    let pews = surface("nave floor", &[part(m::PEWS, 1.0, 0.3)])?;
    let room = Room::extruded_regions(
        &[
            (
                vec![
                    (0.0, 0.0),
                    (30.0, 0.0),
                    (30.0, 2.0),
                    (30.0, 10.0),
                    (30.0, 12.0),
                    (0.0, 12.0),
                ],
                [pews, stone.clone(), brick],
            ),
            (
                vec![(30.0, 2.0), (38.0, 2.0), (38.0, 10.0), (30.0, 10.0)],
                [stone.clone(), stone.clone(), stone],
            ),
        ],
        14.0,
    )?;
    Ok(Space {
        room,
        air: Air::standard(),
        sources: sources(v(31.5, 6.0, 1.6), v(31.5, 8.0, 1.6), v(31.5, 4.0, 1.6)),
        positions: positions(v(22.0, 6.4, 1.5), v(6.0, 5.6, 1.5)),
        non_diffuse: false,
        seam_hz: None,
        max_time_s: 8.0,
        occupancy: "unoccupied pews",
        reference: gated(
            Decay::T30,
            octaves([
                (3.09, 4.4),
                (3.32, 5.88),
                (3.31, 6.83),
                (3.22, 6.47),
                (2.73, 5.91),
                (2.16, 4.19),
            ]),
            "Desarnaulds Table 3, Alberdi 2021, Iannace: five unoccupied churches",
        ),
        notes: vec![
            "the R2 church, its walls exposed brick and its floor covered in pews; piers, window recesses and roof structure by characteristic depth (0.6 m)",
        ],
    })
}

pub fn gothic_cathedral() -> Result<Space, Error> {
    let stone_floor = || surface("stone floor", &[part(m::SANDSTONE, 1.0, 0.0)]);
    let vault = || surface("rib vault", &[relief(m::BRICK, 1.0, 0.8)]);
    let high_walls = || {
        surface(
            "arcade and clerestory",
            &[
                relief(m::BRICK, 0.49, 0.6),
                relief(m::SANDSTONE, 0.30, 0.6),
                relief(m::LEADED_GLAZING, 0.12, 0.3),
                part(m::VELOUR, 0.09, 0.1),
            ],
        )
    };
    let nave_floor = surface(
        "nave floor",
        &[
            part(m::SANDSTONE, 0.25, 0.0),
            part(m::PEWS, 0.6, 0.3),
            part(m::CARPET, 0.15, 0.01),
        ],
    )?;
    let aisle_walls = || {
        surface(
            "aisle walls",
            &[
                relief(m::BRICK, 0.72, 0.6),
                relief(m::LEADED_GLAZING, 0.20, 0.3),
                part(m::VELOUR, 0.08, 0.1),
            ],
        )
    };
    let region = |plan: Vec<(f64, f64)>, materials: [Material; 3], height: f64| Region {
        plan,
        materials,
        height,
    };
    let room = Room::stepped_regions(&[
        region(
            vec![(0.0, 12.0), (70.0, 12.0), (70.0, 26.0), (0.0, 26.0)],
            [nave_floor, vault()?, high_walls()?],
            30.0,
        ),
        region(
            vec![(0.0, 4.0), (70.0, 4.0), (70.0, 12.0), (0.0, 12.0)],
            [stone_floor()?, vault()?, aisle_walls()?],
            13.0,
        ),
        region(
            vec![(0.0, 26.0), (70.0, 26.0), (70.0, 34.0), (0.0, 34.0)],
            [stone_floor()?, vault()?, aisle_walls()?],
            13.0,
        ),
        region(
            vec![(70.0, 12.0), (84.0, 12.0), (84.0, 26.0), (70.0, 26.0)],
            [stone_floor()?, vault()?, high_walls()?],
            30.0,
        ),
        region(
            vec![
                (70.0, 0.0),
                (84.0, 0.0),
                (84.0, 12.0),
                (70.0, 12.0),
                (70.0, 4.0),
            ],
            [stone_floor()?, vault()?, high_walls()?],
            30.0,
        ),
        region(
            vec![
                (70.0, 26.0),
                (84.0, 26.0),
                (84.0, 38.0),
                (70.0, 38.0),
                (70.0, 34.0),
            ],
            [stone_floor()?, vault()?, high_walls()?],
            30.0,
        ),
        region(
            vec![
                (84.0, 12.0),
                (106.0, 12.0),
                (110.0, 15.5),
                (111.5, 19.0),
                (110.0, 22.5),
                (106.0, 26.0),
                (84.0, 26.0),
            ],
            [stone_floor()?, vault()?, high_walls()?],
            30.0,
        ),
    ])?;
    Ok(Space {
        room,
        air: Air::standard(),
        sources: sources(v(78.0, 19.0, 1.7), v(78.0, 23.0, 1.7), v(78.0, 15.0, 1.7)),
        positions: positions(v(60.0, 18.2, 1.5), v(15.0, 20.5, 1.5)),
        non_diffuse: false,
        seam_hz: None,
        max_time_s: 12.0,
        occupancy: "unoccupied chairs in the nave",
        reference: gated(
            Decay::T30,
            octaves([
                (5.8, 7.1),
                (5.7, 8.41),
                (5.2, 7.38),
                (4.4, 6.08),
                (3.6, 4.61),
                (2.7, 3.04),
            ]),
            "Postma & Katz Table 1, Alonso Fig. 4: three cathedrals, nave positions",
        ),
        notes: vec![
            "a nave and choir 30 m high between aisles 13 m high: coupled volumes open under the arcade",
            "sources at the crossing, receivers in the nave; a carpet runner and rugs over 15 % of the nave floor",
            "masonry takes the brick row, measured in churches (Meyer); the page has no row for rough stone walls",
            "banners and hangings over 9 % of the walls; piers, tracery and window recesses by characteristic depth (0.6 m)",
            "the render ends at 10 s with a 0.5 s fade; metrics come from the uncapped render",
        ],
    })
}

/// The stone parish church class: five unoccupied churches.
fn church_range() -> Target {
    octaves([
        (3.09, 4.4),
        (3.32, 5.88),
        (3.31, 6.83),
        (3.22, 6.47),
        (2.73, 5.91),
        (2.16, 4.19),
    ])
}

/// A nave between two lower aisles along x from 0 to `length`, and a chancel of the nave's width and
/// height at x < 0. Materials `[floor, ceiling, walls]`: the aisles' (built twice), the nave's and
/// the chancel's.
fn nave_and_aisles(
    length: f64,
    (nave, aisle): (f64, f64),
    (nave_height, aisle_height): (f64, f64),
    chancel_length: f64,
    aisles: &dyn Fn() -> Result<[Material; 3], Error>,
    [nave_materials, chancel]: [[Material; 3]; 2],
) -> Result<Room, Error> {
    let rectangle =
        |x0: f64, x1: f64, y0: f64, y1: f64| vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1)];
    let region = |plan, materials, height| Region {
        plan,
        materials,
        height,
    };
    Room::stepped_regions(&[
        region(rectangle(0.0, length, 0.0, aisle), aisles()?, aisle_height),
        region(
            rectangle(0.0, length, aisle, aisle + nave),
            nave_materials,
            nave_height,
        ),
        region(
            rectangle(0.0, length, aisle + nave, 2.0 * aisle + nave),
            aisles()?,
            aisle_height,
        ),
        region(
            rectangle(-chancel_length, 0.0, aisle, aisle + nave),
            chancel,
            nave_height,
        ),
    ])
}

pub fn timber_church() -> Result<Space, Error> {
    let floor = surface(
        "nave floor",
        &[
            part(m::WOOD_FLOOR, 0.25, 0.0),
            part(m::PEWS, 0.30, 0.3),
            part(m::MEDIUM_SEATS, 0.30, 0.3),
            part(m::CARPET, 0.15, 0.01),
        ],
    )?;
    let walls = surface(
        "walls",
        &[
            part(m::WOOD_LINING, 0.7, 0.1),
            part(m::GLASS, 0.2, 0.0),
            part(m::VELOUR, 0.1, 0.1),
        ],
    )?;
    let roof = surface("open timber roof", &[relief(m::WOOD_LINING, 1.0, 0.6)])?;
    let section = [
        ((0.0, 0.0), 0),
        ((12.0, 0.0), 1),
        ((12.0, 6.5), 2),
        ((6.0, 11.5), 2),
        ((0.0, 6.5), 1),
    ];
    Ok(space(
        swept(Axis::X, &section, 26.0, 1, vec![floor, walls, roof])?,
        (
            sources(v(3.0, 6.0, 1.6), v(3.0, 4.5, 1.6), v(3.0, 7.5, 1.6)),
            positions(v(10.0, 5.6, 1.2), v(22.0, 6.6, 1.2)),
        ),
        4.0,
        "unoccupied pews, half of them cushioned",
        reported(
            Decay::T30,
            mid((1.4, 2.0)),
            "estimate: a timber church, near the worship design target of 1.5–2.2 s (research notes §14)",
        ),
        vec![
            "a gabled nave in wood, the roof open to its rafters (0.6 m); cushioned pews are medium upholstered seats, a carpet runner, hangings velour",
        ],
    ))
}

pub fn baroque_church() -> Result<Space, Error> {
    let aisles = || -> Result<[Material; 3], Error> {
        Ok([
            surface("chapel floor", &[part(m::TILE, 1.0, 0.0)])?,
            surface("chapel vaults", &[relief(m::PLASTER, 1.0, 1.0)])?,
            surface(
                "chapel walls",
                &[
                    relief(m::PLASTER, 0.6, 1.0),
                    relief(m::GLASS, 0.2, 0.3),
                    relief(m::WOOD_LINING, 0.2, 0.5),
                ],
            )?,
        ])
    };
    let nave = [
        surface("nave floor", &[part(m::TILE, 1.0, 0.0)])?,
        surface("painted vault", &[relief(m::PLASTER, 1.0, 1.2)])?,
        surface(
            "nave walls",
            &[
                relief(m::PLASTER, 0.60, 1.0),
                relief(m::GLASS, 0.15, 0.3),
                relief(m::WOOD_LINING, 0.15, 0.5),
                part(m::VELOUR, 0.10, 0.1),
            ],
        )?,
    ];
    let chancel = [
        surface("chancel floor", &[part(m::TILE, 1.0, 0.0)])?,
        surface("chancel vault", &[relief(m::PLASTER, 1.0, 1.2)])?,
        surface(
            "altar walls",
            &[
                relief(m::PLASTER, 0.7, 1.2),
                relief(m::WOOD_LINING, 0.3, 1.0),
            ],
        )?,
    ];
    let pew = surface(
        "pew rows",
        &[part(m::PEWS, 0.25, 0.3), part(m::WOOD_LINING, 0.75, 0.0)],
    )?;
    let solids = row_of_blocks(
        18,
        Axis::X,
        (6.0, 33.0),
        0.8,
        (6.5, 19.5),
        (0.15, 0.95),
        &pew,
    );
    Ok(space(
        nave_and_aisles(
            40.0,
            (16.0, 5.0),
            (20.0, 10.0),
            10.0,
            &aisles,
            [nave, chancel],
        )?
        .with_solids(&solids)?,
        (
            sources(v(-2.0, 13.0, 1.7), v(-2.0, 10.0, 1.7), v(-2.0, 16.0, 1.7)),
            positions(v(14.0, 12.4, 1.5), v(34.0, 14.0, 1.5)),
        ),
        10.0,
        "unoccupied pews",
        reported(
            Decay::T30,
            church_range(),
            "Desarnaulds Table 3, Alberdi 2021, Iannace: five unoccupied churches (those with a stated volume 4,800–9,100 m³), a smaller class",
        ),
        vec![
            "eighteen pew rows across the nave as solids, each a quarter pew to three quarters frame",
            "a wide nave 20 m high between side chapels 10 m high; stucco, altars, confessionals and the organ by depth (0.5–1.2 m); hangings velour",
            "the render ends at 10 s with a 0.5 s fade; metrics come from the uncapped render",
            "marble floors are the glazed tile row",
        ],
    ))
}

pub fn romanesque_church() -> Result<Space, Error> {
    let aisles = || -> Result<[Material; 3], Error> {
        Ok([
            surface("aisle floor", &[part(m::SANDSTONE, 1.0, 0.0)])?,
            surface("aisle vaults", &[relief(m::SANDSTONE, 1.0, 1.0)])?,
            surface(
                "aisle walls",
                &[
                    relief(m::SANDSTONE, 0.9, 0.6),
                    relief(m::LEADED_GLAZING, 0.1, 0.3),
                ],
            )?,
        ])
    };
    let nave = [
        surface(
            "nave floor",
            &[part(m::PEWS, 0.55, 0.3), part(m::SANDSTONE, 0.45, 0.0)],
        )?,
        surface("barrel vault", &[relief(m::SANDSTONE, 1.0, 1.0)])?,
        surface(
            "arcade and clerestory",
            &[
                relief(m::SANDSTONE, 0.75, 0.6),
                relief(m::LEADED_GLAZING, 0.10, 0.3),
                part(m::WOOD_LINING, 0.10, 0.0),
                part(m::VELOUR, 0.05, 0.1),
            ],
        )?,
    ];
    let chancel = [
        surface("chancel floor", &[part(m::SANDSTONE, 1.0, 0.0)])?,
        surface("chancel vault", &[relief(m::SANDSTONE, 1.0, 1.0)])?,
        surface(
            "chancel walls",
            &[
                relief(m::SANDSTONE, 0.9, 0.6),
                relief(m::LEADED_GLAZING, 0.1, 0.3),
            ],
        )?,
    ];
    let pew = surface(
        "pew rows",
        &[part(m::PEWS, 0.25, 0.3), part(m::WOOD_LINING, 0.75, 0.0)],
    )?;
    let solids = row_of_blocks(
        16,
        Axis::X,
        (5.0, 30.0),
        0.8,
        (6.0, 14.0),
        (0.15, 0.95),
        &pew,
    );
    Ok(space(
        nave_and_aisles(
            36.0,
            (10.0, 5.0),
            (15.0, 7.0),
            9.0,
            &aisles,
            [nave, chancel],
        )?
        .with_solids(&solids)?,
        (
            sources(v(-2.0, 10.0, 1.7), v(-2.0, 7.5, 1.7), v(-2.0, 12.5, 1.7)),
            positions(v(10.0, 9.5, 1.5), v(30.0, 10.6, 1.5)),
        ),
        11.0,
        "unoccupied pews",
        reported(
            Decay::T30,
            church_range(),
            "Desarnaulds Table 3, Alberdi 2021, Iannace: five unoccupied churches (those with a stated volume 4,800–9,100 m³)",
        ),
        vec![
            "sixteen pew rows across the nave as solids",
            "thick stone throughout, the sandstone row; vaults and piers by characteristic depth (0.6–1.0 m)",
        ],
    ))
}

pub fn basilica() -> Result<Space, Error> {
    let aisles = || -> Result<[Material; 3], Error> {
        Ok([
            surface("aisle floor", &[part(m::TILE, 1.0, 0.0)])?,
            surface("aisle ceiling", &[relief(m::PLASTER, 1.0, 1.0)])?,
            surface(
                "aisle walls",
                &[
                    relief(m::BRICK, 0.6, 0.8),
                    relief(m::PLASTER, 0.25, 0.8),
                    relief(m::GLASS, 0.15, 0.3),
                ],
            )?,
        ])
    };
    let nave = [
        surface(
            "nave floor",
            &[part(m::TILE, 0.833, 0.0), part(m::CARPET, 0.167, 0.01)],
        )?,
        surface(
            "coffered ceiling",
            &[
                relief(m::WOOD_LINING, 0.6, 1.2),
                relief(m::PLASTER, 0.4, 1.2),
            ],
        )?,
        surface(
            "colonnade and clerestory",
            &[
                relief(m::BRICK, 0.50, 1.5),
                relief(m::PLASTER, 0.25, 1.5),
                relief(m::GLASS, 0.15, 0.3),
                part(m::VELOUR, 0.10, 0.1),
            ],
        )?,
    ];
    let chancel = [
        surface("apse floor", &[part(m::TILE, 1.0, 0.0)])?,
        surface("apse vault", &[relief(m::PLASTER, 1.0, 1.0)])?,
        surface(
            "apse walls",
            &[relief(m::BRICK, 0.7, 1.0), relief(m::PLASTER, 0.3, 1.0)],
        )?,
    ];
    let pew = surface(
        "pew rows",
        &[part(m::PEWS, 0.25, 0.3), part(m::WOOD_LINING, 0.75, 0.0)],
    )?;
    let solids = row_of_blocks(
        24,
        Axis::X,
        (8.0, 60.0),
        0.9,
        (12.0, 32.0),
        (0.15, 0.95),
        &pew,
    );
    Ok(space(
        nave_and_aisles(
            70.0,
            (24.0, 10.0),
            (24.0, 12.0),
            14.0,
            &aisles,
            [nave, chancel],
        )?
        .with_solids(&solids)?,
        (
            sources(v(-4.0, 22.0, 1.8), v(-4.0, 18.0, 1.8), v(-4.0, 26.0, 1.8)),
            positions(v(18.0, 21.0, 1.5), v(58.0, 23.5, 1.5)),
        ),
        12.0,
        "unoccupied pews",
        reported(
            Decay::T30,
            octaves([
                (5.8, 7.1),
                (5.7, 8.41),
                (5.2, 7.38),
                (4.4, 6.08),
                (3.6, 4.61),
                (2.7, 3.04),
            ]),
            "Postma & Katz Table 1, Alonso Fig. 4: three cathedrals, a neighbour class",
        ),
        vec![
            "twenty-four pew rows across the nave as solids; the nave floor",
            "a brick nave 24 m high between aisles 12 m high, open under a colonnade (1.5 m by depth)",
            "marble floors are the glazed tile row",
            "the render ends at 10 s with a 0.5 s fade; metrics come from the uncapped render",
        ],
    ))
}

pub fn domed_church() -> Result<Space, Error> {
    let floor = surface(
        "floor",
        &[
            part(m::TILE, 0.3, 0.0),
            part(m::PEWS, 0.6, 0.3),
            part(m::CARPET, 0.1, 0.01),
        ],
    )?;
    let walls = surface(
        "walls",
        &[
            relief(m::PLASTER, 0.45, 1.0),
            relief(m::GLASS, 0.15, 0.3),
            relief(m::SANDSTONE, 0.20, 0.6),
            relief(m::WOOD_LINING, 0.10, 0.5),
            part(m::VELOUR, 0.10, 0.1),
        ],
    )?;
    let dome = surface(
        "dome",
        &[relief(m::PLASTER, 0.8, 0.3), relief(m::GLASS, 0.2, 0.3)],
    )?;
    Ok(space(
        domed(16, 16.0, (16.0, 13.0), 5, [floor, walls, dome])?,
        (
            sources(v(6.0, 16.0, 1.7), v(6.0, 13.5, 1.7), v(6.0, 18.5, 1.7)),
            positions(v(15.0, 15.2, 1.5), v(25.5, 17.0, 1.5)),
        ),
        11.0,
        "unoccupied pews",
        reported(
            Decay::T30,
            church_range(),
            "Desarnaulds Table 3, Alberdi 2021, Iannace: five unoccupied churches (one under a dome) (those with a stated volume 4,800–9,100 m³), a smaller class",
        ),
        vec![
            "a central plan of sixteen walls 16 m high under a dome of five rings, windowed over a fifth; altars and niches by depth (1.0 m), hangings velour",
            "the near receiver stands under the dome, where it focuses",
        ],
    ))
}

pub fn concrete_church() -> Result<Space, Error> {
    let size = v(40.0, 24.0, 16.0);
    let walls = surface(
        "board-marked concrete",
        &[
            relief(m::HARD, 0.60, 0.2),
            part(m::BLOCK_COARSE, 0.10, 0.1),
            part(m::GLASS, 0.15, 0.0),
            relief(m::WOOD_LINING, 0.15, 0.3),
        ],
    )?;
    let floor = surface(
        "floor",
        &[
            part(m::TILE, 0.25, 0.0),
            part(m::PEWS, 0.65, 0.3),
            part(m::CARPET, 0.10, 0.01),
        ],
    )?;
    let ceiling = surface(
        "beamed roof",
        &[relief(m::HARD, 0.7, 0.8), relief(m::WOOD_LINING, 0.3, 0.8)],
    )?;
    Ok(space(
        boxed(size, &walls, floor, ceiling)?,
        layout(size),
        8.0,
        "unoccupied pews",
        reported(
            Decay::T30,
            church_range(),
            "Desarnaulds Table 3, Alberdi 2021, Iannace: five unoccupied churches (one brutalist) (those with a stated volume 4,800–9,100 m³), a smaller class",
        ),
        vec![
            "exposed concrete and coarse block under a beamed roof (0.8 m) with timber infill, a glazed strip; marble floor as the glazed tile row",
        ],
    ))
}

pub fn crypt() -> Result<Space, Error> {
    let size = v(20.0, 11.0, 3.4);
    let walls = surface(
        "walls",
        &[part(m::SANDSTONE, 0.8, 0.6), part(m::BRICK, 0.2, 0.6)],
    )?;
    let floor = surface("floor", &[part(m::SANDSTONE, 1.0, 0.0)])?;
    let ceiling = surface(
        "groin vaults",
        &[part(m::SANDSTONE, 0.6, 1.0), part(m::BRICK, 0.4, 1.0)],
    )?;
    Ok(space(
        boxed(size, &walls, floor, ceiling)?,
        layout(size),
        5.0,
        "empty",
        reported(
            Decay::T30,
            mid((2.0, 3.5)),
            "estimate: no published measurement",
        ),
        vec!["low groin vaults on piers, by characteristic depth (0.6–1.0 m), not as geometry"],
    ))
}

pub fn domed_mausoleum() -> Result<Space, Error> {
    let floor = surface("marble floor", &[part(m::TILE, 1.0, 0.0)])?;
    let walls = surface(
        "walls",
        &[
            part(m::TILE, 0.60, 0.0),
            relief(m::SANDSTONE, 0.35, 0.3),
            part(m::WOOD_LINING, 0.05, 0.0),
        ],
    )?;
    let dome = surface(
        "dome",
        &[part(m::SANDSTONE, 0.6, 0.1), part(m::TILE, 0.4, 0.0)],
    )?;
    Ok(space(
        domed(8, 9.0, (14.0, 11.0), 5, [floor, walls, dome])?,
        (
            sources(v(3.5, 9.0, 1.6), v(3.5, 7.5, 1.6), v(3.5, 10.5, 1.6)),
            positions(v(8.0, 8.7, 1.5), v(14.0, 9.8, 1.5)),
        ),
        28.0,
        "empty",
        reported(
            Decay::T30,
            mid((15.0, 15.0)),
            "survey row 19: a domed mausoleum's 15 s door-slam decay, a record claim, not a measurement",
        ),
        vec![
            "an octagon of marble and stone walls 14 m high under a dome of five rings",
            "marble is the glazed tile row; the door wood lining",
            "the render ends at 10 s with a 0.5 s fade; metrics come from the uncapped render",
        ],
    ))
}

pub fn domed_prayer_hall() -> Result<Space, Error> {
    let floor = surface("carpet", &[part(m::CARPET_ON_PAD, 1.0, 0.01)])?;
    let walls = surface(
        "walls",
        &[
            relief(m::PLASTER, 0.45, 0.5),
            part(m::TILE, 0.20, 0.0),
            part(m::GLASS, 0.20, 0.0),
            part(m::WOOD_LINING, 0.05, 0.0),
            part(m::CURTAIN, 0.10, 0.2),
        ],
    )?;
    let dome = surface(
        "dome",
        &[relief(m::PLASTER, 0.85, 0.2), relief(m::GLASS, 0.15, 0.2)],
    )?;
    Ok(space(
        domed(12, 15.0, (12.0, 9.0), 5, [floor, walls, dome])?,
        (
            sources(v(5.0, 15.0, 1.7), v(5.0, 12.5, 1.7), v(5.0, 17.5, 1.7)),
            positions(v(14.0, 14.3, 1.5), v(24.5, 16.2, 1.5)),
        ),
        9.0,
        "empty, carpeted throughout",
        reported(
            Decay::T30,
            mid((1.8, 2.8)),
            "estimate: a carpeted hall under a dome, no published measurement",
        ),
        vec![
            "twelve walls 12 m high under a shallow dome with windows; the whole floor carpeted on underlay, curtains on a tenth of the walls",
        ],
    ))
}

pub fn vaulted_refectory() -> Result<Space, Error> {
    const WIDTH: f64 = 12.0;
    const SPRING: f64 = 7.0;
    let floor = surface(
        "floor",
        &[
            part(m::SANDSTONE, 0.7, 0.0),
            part(m::WOODEN_CHAIRS, 0.3, 0.75),
        ],
    )?;
    let walls = surface(
        "walls",
        &[
            relief(m::PLASTER, 0.30, 0.3),
            relief(m::SANDSTONE, 0.20, 0.3),
            part(m::GLASS, 0.15, 0.0),
            part(m::WOOD_LINING, 0.25, 0.1),
            part(m::VELOUR, 0.10, 0.1),
        ],
    )?;
    let vault = surface(
        "barrel vault",
        &[relief(m::PLASTER, 0.6, 0.2), relief(m::SANDSTONE, 0.4, 0.2)],
    )?;
    let radius = WIDTH / 2.0;
    let mut section = vec![((0.0, 0.0), 0), ((WIDTH, 0.0), 1), ((WIDTH, SPRING), 2)];
    for i in 1..8 {
        let theta = PI * i as f64 / 8.0;
        section.push((
            (radius + radius * theta.cos(), SPRING + radius * theta.sin()),
            2,
        ));
    }
    section.push(((0.0, SPRING), 1));
    Ok(space(
        swept(Axis::X, &section, 36.0, 1, vec![floor, walls, vault])?,
        (
            sources(v(4.0, 6.0, 1.6), v(4.0, 4.5, 1.6), v(4.0, 7.5, 1.6)),
            positions(v(14.0, 5.6, 1.5), v(30.0, 6.6, 1.5)),
        ),
        7.0,
        "long tables and benches, unoccupied",
        reported(
            Decay::T30,
            mid((3.0, 5.0)),
            "estimate: no published measurement",
        ),
        vec![
            "a barrel vault of eight facets on walls 7 m high; tables and benches the wooden chairs row, wainscot wood lining, tapestries velour",
        ],
    ))
}
