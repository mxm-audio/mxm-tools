use super::*;

pub fn vocal_booth() -> Result<Space, Error> {
    let walls = surface(
        "walls",
        &[
            part(m::GLASS_WOOL, 0.35, 0.05),
            part(m::PLASTERBOARD, 0.40, 0.0),
            part(m::GLASS, 0.10, 0.0),
            part(m::WOOD_LINING, 0.15, 0.0),
        ],
    )?;
    let floor = surface("floor", &[part(m::CARPET, 1.0, 0.01)])?;
    let ceiling = surface(
        "ceiling",
        &[
            part(m::GLASS_WOOL, 0.5, 0.05),
            part(m::PLASTERBOARD, 0.5, 0.0),
        ],
    )?;
    Ok(Space {
        room: Room::shoebox(v(2.2, 1.9, 2.4), box_faces(&walls, floor, ceiling))?,
        air: Air::standard(),
        sources: sources(v(0.7, 0.95, 1.6), v(0.7, 0.65, 1.6), v(0.7, 1.25, 1.6)),
        positions: positions(v(1.25, 0.85, 1.6), v(1.75, 1.2, 1.5)),
        non_diffuse: false,
        seam_hz: None,
        max_time_s: 1.2,
        occupancy: "empty booth",
        reference: reported(
            Decay::T20,
            Target::Mean {
                first: 1,
                last: 5,
                range: (0.2, 0.4),
            },
            "EBU Tech 3276 §2.3 specification (a target, not a measurement)",
        ),
        notes: vec!["the window row stands for the control-room glass, wood lining for the door"],
    })
}

pub fn tracking_room() -> Result<Space, Error> {
    let walls = surface(
        "walls",
        &[
            part(m::WOOD_LINING, 0.58, 0.05),
            part(m::PLASTER, 0.28, 0.0),
            part(m::GLASS, 0.08, 0.0),
            part(m::GLASS_WOOL, 0.06, 0.1),
        ],
    )?;
    let floor = surface(
        "floor",
        &[part(m::WOOD_FLOOR, 0.85, 0.0), part(m::CARPET, 0.15, 0.01)],
    )?;
    let ceiling = surface(
        "ceiling",
        &[
            part(m::WOOD_LINING, 0.6, 0.15),
            part(m::GLASS_WOOL, 0.1, 0.05),
            part(m::PLASTERBOARD, 0.3, 0.0),
        ],
    )?;
    let plan = [(0.0, 0.0), (9.5, 0.0), (9.0, 7.2), (0.4, 6.8)];
    Ok(Space {
        room: Room::extruded(&plan, 4.0, [floor, ceiling, walls])?,
        air: Air::standard(),
        sources: sources(v(2.2, 3.4, 1.3), v(2.2, 2.2, 1.3), v(2.2, 4.6, 1.3)),
        positions: positions(v(4.2, 3.1, 1.6), v(7.8, 4.6, 2.0)),
        non_diffuse: false,
        seam_hz: None,
        max_time_s: 2.5,
        occupancy: "empty room",
        reference: reported(
            Decay::T30,
            mid((0.7, 1.2)),
            "estimate (research notes §5): no published measurement",
        ),
        notes: vec![
            "splayed walls, so no two are parallel",
            "bass traps are glass wool over 6 % of the walls",
        ],
    })
}

pub fn drum_room() -> Result<Space, Error> {
    let walls = surface(
        "walls",
        &[
            part(m::PLASTER, 0.35, 0.0),
            part(m::BLOCK_PAINTED, 0.25, 0.02),
            part(m::WOOD_LINING, 0.15, 0.05),
            part(m::GLASS, 0.05, 0.0),
            part(m::GLASS_WOOL, 0.20, 0.05),
        ],
    )?;
    let floor = surface(
        "floor",
        &[part(m::WOOD_FLOOR, 0.9, 0.0), part(m::CARPET, 0.1, 0.01)],
    )?;
    let ceiling = surface(
        "ceiling",
        &[part(m::PLASTER, 0.7, 0.0), part(m::WOOD_LINING, 0.3, 0.2)],
    )?;
    Ok(Space {
        room: Room::shoebox(v(12.0, 10.0, 5.0), box_faces(&walls, floor, ceiling))?,
        air: Air::standard(),
        sources: sources(v(3.0, 5.2, 1.0), v(3.0, 4.2, 1.0), v(3.0, 6.2, 1.0)),
        positions: positions(v(5.0, 4.8, 1.8), v(10.6, 6.6, 3.2)),
        non_diffuse: false,
        seam_hz: None,
        max_time_s: 3.0,
        occupancy: "empty room, a drum rug",
        reference: reported(
            Decay::T30,
            mid((0.8, 1.5)),
            "estimate (research notes §6): no published measurement",
        ),
        notes: vec!["parallel hard walls, the class's flutter"],
    })
}

pub fn control_room() -> Result<Space, Error> {
    let walls = surface(
        "walls",
        &[
            part(m::MINERAL_BOARD, 0.40, 0.15),
            part(m::BASS_TRAP, 0.15, 0.3),
            part(m::PERFORATED_WOOD, 0.15, 0.1),
            part(m::GLASS, 0.15, 0.0),
            part(m::PLASTERBOARD, 0.15, 0.0),
        ],
    )?;
    let floor = surface(
        "floor",
        &[
            part(m::WOOD_FLOOR, 0.5, 0.0),
            part(m::CARPET, 0.3, 0.01),
            part(m::SEATS, 0.2, 0.6),
        ],
    )?;
    let ceiling = surface(
        "ceiling",
        &[
            part(m::MINERAL_BOARD, 0.6, 0.2),
            part(m::PLASTERBOARD, 0.4, 0.0),
        ],
    )?;
    let plan = [(0.0, 0.0), (6.2, 0.3), (6.2, 4.9), (0.0, 5.2)];
    Ok(space(
        Room::extruded(&plan, 3.0, [floor, ceiling, walls])?,
        (
            sources(v(0.9, 2.6, 1.3), v(1.1, 1.35, 1.3), v(1.1, 3.85, 1.3)),
            positions(v(3.2, 2.45, 1.2), v(5.3, 2.9, 1.2)),
        ),
        1.2,
        "a desk and a sofa",
        reported(
            Decay::T20,
            Target::Mean {
                first: 1,
                last: 5,
                range: (0.2, 0.4),
            },
            "EBU Tech 3276 §2.3 specification for listening rooms (a target, not a measurement)",
        ),
        vec![
            "splayed side walls, symmetric about the listening axis; the sources stand for the monitors",
            "bass traps are the plywood-over-mineral-fibre row, the sofa upholstered seating",
        ],
    ))
}

pub fn amp_booth() -> Result<Space, Error> {
    let size = v(1.6, 1.4, 2.2);
    let walls = surface(
        "walls",
        &[
            part(m::MINERAL_BOARD, 0.6, 0.05),
            part(m::PLASTERBOARD, 0.3, 0.0),
            part(m::GLASS, 0.1, 0.0),
        ],
    )?;
    let floor = surface("floor", &[part(m::CARPET, 1.0, 0.01)])?;
    let ceiling = surface(
        "ceiling",
        &[
            part(m::MINERAL_BOARD, 0.6, 0.05),
            part(m::PLASTERBOARD, 0.4, 0.0),
        ],
    )?;
    Ok(space(
        boxed(size, &walls, floor, ceiling)?,
        layout(size),
        0.6,
        "empty booth",
        reported(
            Decay::T20,
            mid((0.05, 0.2)),
            "estimate: an isolation booth, deader than the vocal booth's specification",
        ),
        vec![
            "absorbent panels are the mineral-fibre board row; the window row stands for the door glass",
        ],
    ))
}

pub fn piano_room() -> Result<Space, Error> {
    let size = v(7.2, 5.6, 3.6);
    let walls = surface(
        "walls",
        &[
            part(m::PLASTER, 0.45, 0.0),
            part(m::WOOD_LINING, 0.30, 0.3),
            part(m::CURTAIN, 0.15, 0.2),
            part(m::GLASS, 0.10, 0.0),
        ],
    )?;
    let floor = surface(
        "floor",
        &[part(m::WOOD_FLOOR, 0.85, 0.0), part(m::CARPET, 0.15, 0.01)],
    )?;
    let ceiling = surface(
        "ceiling",
        &[
            part(m::PLASTERBOARD, 0.6, 0.0),
            part(m::PERFORATED_WOOD, 0.4, 0.1),
        ],
    )?;
    Ok(space(
        boxed(size, &walls, floor, ceiling)?,
        layout(size),
        2.5,
        "empty room, a piano",
        reported(
            Decay::T30,
            octaves([
                (0.7, 0.7),
                (0.6, 0.6),
                (0.6, 0.8),
                (0.6, 0.9),
                (0.7, 1.0),
                (0.6, 0.9),
            ]),
            "Osman Table 1.3 (secondary): two unoccupied teaching studios",
        ),
        vec!["wooden diffusers 0.3 m deep on 30 % of the walls, curtains on 15 %"],
    ))
}

pub fn string_room() -> Result<Space, Error> {
    let walls = surface(
        "walls",
        &[
            part(m::WOOD_LINING, 0.45, 0.4),
            part(m::PLASTER, 0.25, 0.2),
            part(m::GLASS, 0.05, 0.0),
            part(m::GLASS_WOOL, 0.15, 0.1),
            part(m::CURTAIN, 0.10, 0.2),
        ],
    )?;
    let floor = surface("floor", &[part(m::WOOD_FLOOR, 1.0, 0.0)])?;
    let ceiling = surface(
        "ceiling",
        &[
            part(m::WOOD_LINING, 0.4, 0.5),
            part(m::PLASTERBOARD, 0.4, 0.5),
            part(m::MINERAL_BOARD, 0.2, 0.0),
        ],
    )?;
    let plan = [(0.0, 0.0), (11.5, 0.4), (11.0, 8.4), (0.3, 8.0)];
    Ok(space(
        Room::extruded(&plan, 5.2, [floor, ceiling, walls])?,
        (
            sources(v(2.5, 4.2, 1.3), v(2.5, 3.0, 1.3), v(2.5, 5.4, 1.3)),
            positions(v(5.0, 3.9, 1.6), v(9.2, 4.7, 2.2)),
        ),
        3.0,
        "empty room",
        reported(
            Decay::T30,
            octaves([
                (0.9, 1.1),
                (0.7, 1.0),
                (0.8, 1.0),
                (0.7, 1.1),
                (0.7, 1.2),
                (0.7, 1.2),
            ]),
            "Osman Table 1.3 (secondary): three choral rehearsal rooms, a neighbour class",
        ),
        vec![
            "splayed walls lined in wood 0.4 m deep, glass wool panels on 15 %, curtains on 10 %; a coffered ceiling 0.5 m deep with absorbent panels",
        ],
    ))
}

pub fn stone_live_room() -> Result<Space, Error> {
    let walls = surface(
        "walls",
        &[
            part(m::SANDSTONE, 0.50, 0.25),
            part(m::BRICK, 0.25, 0.1),
            part(m::WOOD_LINING, 0.10, 0.0),
            part(m::GLASS, 0.10, 0.0),
            part(m::GLASS_WOOL, 0.05, 0.1),
        ],
    )?;
    let floor = surface(
        "floor",
        &[part(m::WOOD_FLOOR, 0.7, 0.0), part(m::CARPET, 0.3, 0.01)],
    )?;
    let ceiling = surface(
        "ceiling",
        &[
            part(m::WOOD_LINING, 0.6, 0.3),
            part(m::PLASTERBOARD, 0.4, 0.0),
        ],
    )?;
    let plan = [(0.0, 0.0), (10.0, 0.0), (9.6, 7.8), (0.3, 8.2)];
    Ok(space(
        Room::extruded(&plan, 5.0, [floor, ceiling, walls])?,
        (
            sources(v(2.2, 4.0, 1.3), v(2.2, 2.8, 1.3), v(2.2, 5.2, 1.3)),
            positions(v(4.6, 3.7, 1.6), v(8.2, 4.6, 2.0)),
        ),
        3.5,
        "empty room, rugs",
        reported(
            Decay::T30,
            mid((1.2, 1.8)),
            "estimate: above the tracking room's 0.7–1.2 s (research notes §5)",
        ),
        vec![
            "rough stone walls take the sandstone row, the page's nearest to rough stone (0.25 m relief)",
        ],
    ))
}

pub fn dead_studio() -> Result<Space, Error> {
    let size = v(9.0, 7.0, 3.5);
    let walls = surface(
        "walls",
        &[
            part(m::VELOUR, 0.5, 0.1),
            part(m::GLASS_WOOL, 0.3, 0.0),
            part(m::WOOD_LINING, 0.1, 0.0),
            part(m::GLASS, 0.1, 0.0),
        ],
    )?;
    let floor = surface("floor", &[part(m::CARPET_ON_PAD, 1.0, 0.01)])?;
    let ceiling = surface(
        "ceiling",
        &[
            part(m::GLASS_WOOL, 0.7, 0.0),
            part(m::PLASTERBOARD, 0.3, 0.0),
        ],
    )?;
    Ok(space(
        boxed(size, &walls, floor, ceiling)?,
        layout(size),
        1.2,
        "empty room",
        reported(
            Decay::T20,
            mid((0.2, 0.4)),
            "estimate: a dead studio of drapes and carpet, no published measurement",
        ),
        vec!["drapes over half the walls, glass wool over most of the ceiling, carpet on underlay"],
    ))
}

pub fn large_live_room() -> Result<Space, Error> {
    let size = v(14.0, 11.0, 6.0);
    let walls = surface(
        "walls",
        &[
            part(m::WOOD_LINING, 0.40, 0.4),
            part(m::PLASTER, 0.35, 0.4),
            part(m::GLASS, 0.05, 0.0),
            part(m::GLASS_WOOL, 0.20, 0.1),
        ],
    )?;
    let floor = surface("floor", &[part(m::WOOD_FLOOR, 1.0, 0.0)])?;
    let ceiling = surface(
        "ceiling",
        &[
            part(m::WOOD_LINING, 0.4, 0.6),
            part(m::PLASTERBOARD, 0.6, 0.6),
        ],
    )?;
    Ok(space(
        boxed(size, &walls, floor, ceiling)?,
        layout(size),
        3.5,
        "empty room",
        reported(
            Decay::T30,
            mid((1.2, 1.6)),
            "estimate: above the tracking room's 0.7–1.2 s (research notes §5)",
        ),
        vec!["pilasters and diffusers by depth (0.4 m), a coffered ceiling 0.6 m deep"],
    ))
}

pub fn tight_drum_room() -> Result<Space, Error> {
    let size = v(4.8, 4.2, 2.8);
    let walls = surface(
        "walls",
        &[
            part(m::GLASS_WOOL, 0.25, 0.1),
            part(m::WOOD_LINING, 0.45, 0.0),
            part(m::PLASTERBOARD, 0.20, 0.0),
            part(m::GLASS, 0.10, 0.0),
        ],
    )?;
    let floor = surface(
        "floor",
        &[part(m::WOOD_FLOOR, 0.6, 0.0), part(m::CARPET, 0.4, 0.01)],
    )?;
    let ceiling = surface(
        "ceiling",
        &[
            part(m::GLASS_WOOL, 0.5, 0.0),
            part(m::PLASTERBOARD, 0.5, 0.0),
        ],
    )?;
    Ok(space(
        boxed(size, &walls, floor, ceiling)?,
        layout(size),
        1.5,
        "a drum kit on a rug",
        reported(
            Decay::T30,
            mid((0.3, 0.5)),
            "estimate: a damped drum room, no published measurement",
        ),
        vec![
            "glass wool over a quarter of the walls and half the ceiling, a drum rug over 40 % of the floor",
        ],
    ))
}

pub fn rehearsal_room() -> Result<Space, Error> {
    let size = v(6.5, 4.6, 2.8);
    let walls = surface(
        "walls",
        &[
            part(m::BLOCK_PAINTED, 0.65, 0.0),
            part(m::MINERAL_BOARD, 0.15, 0.05),
            part(m::VELOUR, 0.10, 0.1),
            part(m::WOOD_LINING, 0.10, 0.0),
        ],
    )?;
    let floor = surface("floor", &[part(m::CARPET_THIN, 1.0, 0.01)])?;
    let ceiling = surface(
        "ceiling",
        &[
            part(m::PLASTERBOARD, 0.8, 0.0),
            part(m::MINERAL_BOARD, 0.2, 0.0),
        ],
    )?;
    Ok(space(
        boxed(size, &walls, floor, ceiling)?,
        layout(size),
        2.0,
        "amplifiers and a kit, no band",
        reported(
            Decay::T30,
            octaves([
                (0.6, 1.1),
                (0.4, 1.1),
                (0.3, 0.9),
                (0.2, 1.0),
                (0.2, 1.1),
                (0.2, 1.0),
            ]),
            "Osman Table 1.3 (secondary): two ensemble rooms",
        ),
        vec![
            "painted block walls with foam panels (the mineral-fibre board row) on 15 %, a curtain, thin carpet",
        ],
    ))
}

pub fn broadcast_studio() -> Result<Space, Error> {
    let size = v(8.5, 6.2, 3.3);
    let walls = surface(
        "walls",
        &[
            part(m::PERFORATED_WOOD, 0.50, 0.05),
            part(m::MINERAL_BOARD, 0.10, 0.0),
            part(m::GLASS, 0.15, 0.0),
            part(m::PLASTERBOARD, 0.25, 0.0),
        ],
    )?;
    let floor = surface(
        "floor",
        &[
            part(m::CARPET, 0.8, 0.01),
            part(m::WOODEN_CHAIRS, 0.2, 0.75),
        ],
    )?;
    let ceiling = surface(
        "ceiling",
        &[
            part(m::MINERAL_BOARD, 0.8, 0.0),
            part(m::PLASTERBOARD, 0.2, 0.0),
        ],
    )?;
    Ok(space(
        boxed(size, &walls, floor, ceiling)?,
        layout(size),
        1.2,
        "a presenters' desk",
        reported(
            Decay::T20,
            mid((0.25, 0.45)),
            "estimate: a treated speech studio, no published measurement",
        ),
        vec!["perforated wood panels over half the walls, a mineral-fibre ceiling"],
    ))
}
