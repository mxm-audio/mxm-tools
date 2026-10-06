use super::*;

pub fn car_cabin() -> Result<Space, Error> {
    // No car-interior absorption was found (research page §8.1, gap 2). The trim is a blend of
    // generic rows whose fractions follow the two measured cabins' decay shape: long at 125 Hz,
    // under 0.1 s from 1 kHz.
    let trim = surface(
        "cabin trim",
        &[
            part(m::CARPET, 0.45, 0.02),
            part(m::VELOUR, 0.30, 0.05),
            part(m::SEATS, 0.10, 0.45),
            part(m::TILE, 0.15, 0.0),
        ],
    )?;
    Ok(Space {
        room: Room::shoebox(v(2.4, 1.45, 1.15), std::array::from_fn(|_| trim.clone()))?,
        air: Air::standard(),
        sources: sources(v(0.3, 0.725, 0.8), v(0.5, 0.25, 0.5), v(0.5, 1.2, 0.5)),
        positions: positions(v(1.15, 0.45, 0.85), v(2.0, 0.725, 0.85)),
        non_diffuse: false,
        seam_hz: None,
        max_time_s: 1.0,
        occupancy: "windows closed, no passengers",
        reference: Reference {
            slack_s: 0.02,
            ..gated(
                Decay::T20,
                octaves([
                    (0.37, 0.59),
                    (0.15, 0.24),
                    (0.08, 0.15),
                    (0.06, 0.08),
                    (0.05, 0.09),
                    (0.05, 0.07),
                ]),
                "Soeta Figs. 4–6: two cars, all seats, read off figures (±0.02 s)",
            )
        },
        notes: vec![
            "a box cabin; seats are not modelled as solids",
            "glazing is the glazed-tile row: small stiff panes, not a room's window panel",
            "the sources stand for the dashboard centre and the front door loudspeakers",
        ],
    })
}

pub fn cargo_van() -> Result<Space, Error> {
    let size = v(3.2, 1.75, 1.4);
    let shell = surface(
        "load space",
        &[part(m::HARD, 0.6, 0.05), part(m::WOOD_LINING, 0.4, 0.0)],
    )?;
    let floor = surface("floor", &[part(m::WOOD_FLOOR, 1.0, 0.0)])?;
    let crate_stack = surface("crates", &[part(m::WOOD_LINING, 1.0, 0.05)])?;
    let solids = [
        block((2.75, 3.05), (0.15, 0.75), (0.08, 0.55), &crate_stack),
        block((2.75, 3.05), (1.0, 1.6), (0.08, 0.55), &crate_stack),
    ];
    Ok(space(
        boxed(size, &shell, floor, shell.clone())?.with_solids(&solids)?,
        layout(size),
        1.2,
        "empty load space, doors closed",
        reported(
            Decay::T20,
            mid((0.3, 0.7)),
            "estimate: no published measurement",
        ),
        vec![
            "no row for sheet steel: the hard row stands for the bare panels, ribbed 5 cm; plywood lining on 40 %, two crates as solids",
        ],
    ))
}

pub fn city_bus() -> Result<Space, Error> {
    let size = v(11.0, 2.45, 2.15);
    let walls = surface(
        "sides",
        &[
            part(m::GLASS, 0.45, 0.2),
            part(m::HARD, 0.35, 0.3),
            part(m::MEDIUM_SEATS, 0.2, 0.4),
        ],
    )?;
    let floor = surface("floor", &[part(m::HARD, 1.0, 0.0)])?;
    let ceiling = surface(
        "ceiling",
        &[part(m::HARD, 0.8, 0.1), part(m::GLASS_WOOL, 0.2, 0.0)],
    )?;
    let seat = surface(
        "seat rows",
        &[part(m::MEDIUM_SEATS, 0.25, 0.4), part(m::HARD, 0.75, 0.0)],
    )?;
    let mut solids = row_of_blocks(
        8,
        Axis::X,
        (3.0, 10.4),
        0.9,
        (0.12, 0.62),
        (0.1, 1.0),
        &seat,
    );
    solids.extend(row_of_blocks(
        8,
        Axis::X,
        (3.0, 10.4),
        0.9,
        (1.83, 2.33),
        (0.1, 1.0),
        &seat,
    ));
    Ok(space(
        boxed(size, &walls, floor, ceiling)?.with_solids(&solids)?,
        layout(size),
        1.5,
        "no passengers",
        reported(
            Decay::T20,
            mid((0.4, 0.8)),
            "estimate: no published measurement",
        ),
        vec![
            "seats are the medium upholstered row over 45 % of the floor; windows the window glass row over 45 % of the sides, seat backs medium upholstered seats on a fifth, handrails and panels by depth",
            "sixteen seat blocks as solids down both sides, each a quarter upholstery to three quarters frame; the floor no longer stands in for them",
        ],
    ))
}

pub fn train_carriage() -> Result<Space, Error> {
    let size = v(20.0, 2.8, 2.3);
    let walls = surface(
        "sides",
        &[
            part(m::GLASS, 0.40, 0.3),
            part(m::PLASTERBOARD, 0.35, 0.4),
            part(m::SEATS, 0.25, 0.4),
        ],
    )?;
    let floor = surface("floor", &[part(m::CARPET, 1.0, 0.01)])?;
    let ceiling = surface(
        "ceiling",
        &[
            part(m::PERFORATED_WOOD, 0.3, 0.05),
            part(m::PLASTERBOARD, 0.7, 0.0),
        ],
    )?;
    let seat = surface(
        "seat bays",
        &[part(m::SEATS, 0.25, 0.4), part(m::PLASTERBOARD, 0.75, 0.0)],
    )?;
    let mut solids = row_of_blocks(
        7,
        Axis::X,
        (4.5, 19.0),
        1.0,
        (0.15, 0.75),
        (0.1, 1.1),
        &seat,
    );
    solids.extend(row_of_blocks(
        7,
        Axis::X,
        (4.5, 19.0),
        1.0,
        (2.05, 2.65),
        (0.1, 1.1),
        &seat,
    ));
    Ok(space(
        boxed(size, &walls, floor, ceiling)?.with_solids(&solids)?,
        layout(size),
        1.5,
        "no passengers",
        reported(
            Decay::T20,
            mid((0.3, 0.6)),
            "estimate: no published measurement",
        ),
        vec![
            "upholstered seating over 60 % of the floor, carpet between; trim panels and luggage racks the plasterboard row, seat backs against the sides upholstered seating (0.4 m deep)",
            "fourteen seat bays as solids down both sides, each a quarter upholstery to three quarters frame; the floor is carpet alone",
        ],
    ))
}

pub fn lift_car() -> Result<Space, Error> {
    let size = v(1.6, 1.4, 2.3);
    let walls = surface(
        "walls",
        &[part(m::HARD, 0.75, 0.0), part(m::PLATE_GLASS, 0.25, 0.0)],
    )?;
    let floor = surface("floor", &[part(m::TILE, 1.0, 0.0)])?;
    let ceiling = surface(
        "ceiling",
        &[
            part(m::HARD, 0.7, 0.05),
            part(m::PERFORATED_METAL, 0.3, 0.05),
        ],
    )?;
    Ok(space(
        boxed(size, &walls, floor, ceiling)?,
        layout(size),
        2.5,
        "empty, doors closed",
        reported(
            Decay::T30,
            mid((0.4, 1.2)),
            "estimate: no published measurement",
        ),
        vec![
            "no row for steel: the hard row stands for the panels; a mirror is the plate glass row, the vent perforated metal",
        ],
    ))
}
