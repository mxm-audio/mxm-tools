use super::*;

pub fn concrete_stairwell() -> Result<Space, Error> {
    let concrete = surface("painted concrete", &[part(m::HARD, 1.0, 0.0)])?;
    let flight = surface("stair flight", &[part(m::HARD, 1.0, 0.17)])?;
    // Seven flights, alternating half-widths and directions, each a 0.25 m slab rising 1.6 m over
    // 4.4 m and clear of its neighbours; landings are left out.
    let prisms: Vec<Prism> = (0..7)
        .map(|k| {
            let z0 = 0.4 + 1.6 * k as f64;
            let (low, high) = if k % 2 == 0 { (1.2, 5.6) } else { (5.6, 1.2) };
            Prism {
                section: vec![
                    (low, z0),
                    (high, z0 + 1.6),
                    (high, z0 + 1.85),
                    (low, z0 + 0.25),
                ],
                y: if k % 2 == 0 {
                    (0.1, 1.40)
                } else {
                    (1.58, 2.88)
                },
                material: flight.clone(),
            }
        })
        .collect();
    let room = Room::box_with_prisms(
        v(0.0, 0.0, 0.0),
        v(6.83, 2.98, 12.71),
        std::array::from_fn(|_| concrete.clone()),
        &prisms,
    )?;
    Ok(Space {
        room,
        air: Air::standard(),
        sources: sources(v(0.6, 1.49, 1.72), v(0.6, 0.7, 1.72), v(0.6, 2.3, 1.72)),
        positions: positions(v(2.6, 2.3, 1.72), v(6.2, 1.49, 11.0)),
        non_diffuse: true,
        seam_hz: None,
        max_time_s: 9.0,
        occupancy: "empty",
        reference: reported(
            Decay::T30,
            Target::Octaves([Some((5.3, 5.3)), None, None, None, None, None]),
            "Kirsch et al.: one staircase, 5.3 s at 125 Hz (8 kHz end point out of range)",
        ),
        notes: vec![
            "the measured staircase's shaft, 2.98 × 6.83 × 12.71 m, with floating flights",
            "near: ground floor, 2 m from the source as measured; far: the top landing",
        ],
    })
}

pub fn empty_warehouse() -> Result<Space, Error> {
    let size = v(60.0, 40.0, 10.0);
    let walls = surface(
        "walls",
        &[
            part(m::BLOCK_PAINTED, 0.7, 0.3),
            part(m::HARD, 0.2, 0.0),
            part(m::GLASS, 0.1, 0.0),
        ],
    )?;
    let floor = surface("floor", &[part(m::CONCRETE_FLOOR, 1.0, 0.0)])?;
    let ceiling = surface(
        "roof",
        &[
            part(m::HARD, 0.67, 1.5),
            part(m::GLASS, 0.15, 0.0),
            part(m::GLASS_WOOL, 0.18, 0.0),
        ],
    )?;
    Ok(space(
        boxed(size, &walls, floor, ceiling)?,
        layout(size),
        10.0,
        "empty",
        reported(
            Decay::T30,
            mid((2.7, 5.1)),
            "Kanev 2021 Table 5: eight enclosed shopping halls without absorbers, a neighbour (no warehouse measurement found)",
        ),
        vec![
            "no row for steel: the hard row stands for the doors and the roof deck; trusses and purlins 1.5 m deep, pilasters 0.3 m; faced insulation between purlins the glass wool row",
        ],
    ))
}

pub fn sawtooth_factory() -> Result<Space, Error> {
    let ends = surface("gable walls", &[relief(m::BLOCK_PAINTED, 1.0, 0.3)])?;
    let floor = surface(
        "floor",
        &[
            part(m::CONCRETE_FLOOR, 0.7, 0.0),
            relief(m::HARD, 0.2, 1.5),
            relief(m::WOOD_LINING, 0.1, 1.0),
        ],
    )?;
    let walls = surface(
        "walls",
        &[relief(m::BLOCK_PAINTED, 0.7, 0.3), part(m::GLASS, 0.3, 0.0)],
    )?;
    let roof = surface(
        "roof slopes",
        &[
            relief(m::HARD, 0.55, 0.5),
            part(m::WOOD_LINING, 0.20, 0.0),
            part(m::GLASS_WOOL, 0.25, 0.0),
        ],
    )?;
    let glazing = surface("north lights", &[part(m::GLASS, 1.0, 0.0)])?;
    // Cross-section in (x, z), swept across y: five roof slopes rising 3 m toward −x,
    // a vertical glazed light between each pair.
    let mut section = vec![((0.0, 0.0), 1), ((50.0, 0.0), 2)];
    for tooth in (0..5).rev() {
        let x = 10.0 * tooth as f64;
        section.push(((x + 10.0, 10.0), 3));
        if tooth > 0 {
            section.push(((x, 13.0), 4));
        }
    }
    section.push(((0.0, 13.0), 2));
    Ok(space(
        swept(
            Axis::Y,
            &section,
            32.0,
            0,
            vec![ends, floor, walls, roof, glazing],
        )?,
        (
            sources(v(8.0, 16.0, 1.6), v(8.0, 13.0, 1.6), v(8.0, 19.0, 1.6)),
            positions(v(20.0, 15.0, 1.6), v(42.0, 17.5, 1.6)),
        ),
        6.0,
        "machines, no work",
        reported(
            Decay::T30,
            mid((3.0, 5.0)),
            "estimate: no published measurement",
        ),
        vec![
            "a sawtooth roof of five slopes with four vertical north lights, insulated between purlins (the glass wool row); machines are the hard row 1.5 m deep over a fifth of the floor, pallets wood lining",
        ],
    ))
}

pub fn aircraft_hangar() -> Result<Space, Error> {
    let size = v(90.0, 70.0, 24.0);
    let walls = surface(
        "walls",
        &[
            relief(m::HARD, 0.45, 0.5),
            part(m::BLOCK_PAINTED, 0.25, 0.0),
            part(m::GLASS, 0.15, 0.0),
            part(m::GLASS_WOOL, 0.15, 0.0),
        ],
    )?;
    let floor = surface("floor", &[part(m::CONCRETE_FLOOR, 1.0, 0.0)])?;
    let ceiling = surface(
        "roof",
        &[
            relief(m::HARD, 0.65, 3.0),
            part(m::GLASS, 0.15, 0.0),
            part(m::GLASS_WOOL, 0.20, 0.0),
        ],
    )?;
    Ok(space(
        boxed(size, &walls, floor, ceiling)?,
        layout(size),
        18.0,
        "empty",
        reported(
            Decay::T30,
            mid((4.0, 8.0)),
            "estimate: no published measurement",
        ),
        vec![
            "no row for steel: the hard row stands for the cladding (0.5 m ribs) and the roof, its trusses 3 m deep; insulation liners the glass wool row",
            "the render ends at 10 s with a 0.5 s fade; metrics come from the uncapped render",
        ],
    ))
}

pub fn plant_room() -> Result<Space, Error> {
    let size = v(14.0, 9.0, 4.5);
    let walls = surface(
        "walls",
        &[
            part(m::BLOCK_PAINTED, 0.60, 0.5),
            part(m::HARD, 0.28, 0.5),
            part(m::MINERAL_BOARD, 0.12, 0.0),
        ],
    )?;
    let floor = surface("floor", &[part(m::CONCRETE_FLOOR, 1.0, 0.0)])?;
    let ceiling = surface("soffit and ducts", &[part(m::HARD, 1.0, 0.6)])?;
    let plant = surface("boilers and pumps", &[part(m::HARD, 1.0, 0.3)])?;
    let duct = surface("ducts", &[part(m::HARD, 1.0, 0.2)])?;
    let solids = [
        block((4.0, 6.0), (6.5, 8.5), (0.2, 2.2), &plant),
        block((8.0, 9.5), (1.0, 2.5), (0.2, 1.2), &plant),
        block((3.0, 12.0), (4.2, 5.0), (3.4, 4.0), &duct),
    ];
    Ok(space(
        boxed(size, &walls, floor, ceiling)?.with_solids(&solids)?,
        layout(size),
        5.0,
        "boilers and pumps, idle",
        reported(
            Decay::T30,
            mid((1.2, 2.5)),
            "estimate: no published measurement",
        ),
        vec![
            "two plant blocks and a duct run as solids; noise-control panels on an eighth of the walls, pipework by depth",
            "boilers and pumps are the hard row 1.5 m deep over a quarter of the floor; pipework and ducts by depth; noise-control panels on an eighth of the walls",
        ],
    ))
}

pub fn mill_loft() -> Result<Space, Error> {
    let size = v(40.0, 14.0, 4.2);
    let walls = surface(
        "walls",
        &[
            part(m::BRICK, 0.60, 0.3),
            part(m::GLASS, 0.35, 0.3),
            part(m::WOOD_LINING, 0.05, 0.0),
        ],
    )?;
    let floor = surface("floor", &[part(m::WOOD_FLOOR, 1.0, 0.0)])?;
    let ceiling = surface(
        "joists",
        &[part(m::WOOD_LINING, 0.7, 0.4), part(m::PLASTER, 0.3, 0.4)],
    )?;
    let column = surface("cast-iron columns", &[part(m::HARD, 1.0, 0.1)])?;
    let solids = row_of_blocks(
        6,
        Axis::X,
        (6.0, 36.0),
        0.35,
        (6.8, 7.15),
        (0.25, 3.9),
        &column,
    );
    Ok(space(
        boxed(size, &walls, floor, ceiling)?.with_solids(&solids)?,
        layout(size),
        4.0,
        "empty floor",
        reported(
            Decay::T30,
            mid((1.5, 2.5)),
            "estimate: no published measurement",
        ),
        vec![
            "a long brick floor with windows down both sides; six cast-iron columns as solids, joists by depth (0.4 m)",
            "a long brick floor with windows down both sides; window reveals and piers (0.3 m), cast-iron columns and joists (0.4 m) by depth",
        ],
    ))
}

pub fn shipping_container() -> Result<Space, Error> {
    let size = v(5.9, 2.35, 2.39);
    let steel = surface("corrugated steel", &[part(m::HARD, 1.0, 0.04)])?;
    let floor = surface("plywood floor", &[part(m::WOOD_FLOOR, 1.0, 0.0)])?;
    Ok(space(
        boxed(size, &steel, floor, steel.clone())?,
        layout(size),
        3.5,
        "empty, doors closed",
        reported(
            Decay::T30,
            mid((0.8, 2.5)),
            "estimate: no published measurement",
        ),
        vec!["no row for steel: the hard row stands for the corrugated panels (4 cm)"],
    ))
}
