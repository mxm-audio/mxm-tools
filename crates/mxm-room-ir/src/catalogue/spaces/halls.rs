use super::*;

pub fn recital_hall() -> Result<Space, Error> {
    let walls = surface(
        "hall walls",
        &[
            part(m::PLASTERBOARD, 0.60, 1.5),
            part(m::WOOD_LINING, 0.30, 1.0),
            part(m::GLASS, 0.1, 0.5),
        ],
    )?;
    let seats = surface(
        "audience floor",
        &[part(m::SEATS, 0.85, 0.45), part(m::CARPET, 0.15, 0.01)],
    )?;
    let ceiling = surface(
        "hall ceiling",
        &[
            part(m::PLASTERBOARD, 0.5, 1.5),
            part(m::WOOD_LINING, 0.5, 1.5),
        ],
    )?;
    let stage = surface("stage", &[part(m::WOOD_FLOOR, 1.0, 0.0)])?;
    let gallery = surface(
        "side galleries",
        &[part(m::SEATS, 0.15, 0.45), part(m::WOOD_LINING, 0.85, 0.3)],
    )?;
    // The long axis is y. A raised stage and two side galleries are solids: a flat box let sound
    // skim between parallel surfaces at low frequencies, and 125 Hz decayed for 2.5 s.
    let pilaster = surface("pilasters", &[part(m::PLASTER, 1.0, 0.0)])?;
    let rib = surface("ceiling ribs", &[part(m::WOOD_LINING, 1.0, 0.0)])?;
    let mut prisms = vec![
        Prism {
            section: vec![(1.0, 0.0), (19.0, 0.0), (19.0, 0.8), (1.0, 0.8)],
            y: (0.5, 6.0),
            material: stage,
        },
        Prism {
            section: vec![(0.5, 4.5), (2.5, 4.5), (2.5, 5.3), (0.5, 5.3)],
            y: (7.0, 31.5),
            material: gallery.clone(),
        },
        Prism {
            section: vec![(17.5, 4.5), (19.5, 4.5), (19.5, 5.3), (17.5, 5.3)],
            y: (7.0, 31.5),
            material: gallery,
        },
    ];
    // Fourteen pilasters a side above the galleries and ten ceiling ribs, as solids. They clear the
    // floor, so they do not grid it into hundreds of faces.
    for i in 0..14 {
        let y0 = 7.0 + 1.7 * i as f64;
        for x in [0.08, 19.32] {
            prisms.push(Prism {
                section: vec![(x, 5.6), (x + 0.6, 5.6), (x + 0.6, 10.5), (x, 10.5)],
                y: (y0, y0 + 0.6),
                material: pilaster.clone(),
            });
        }
    }
    for i in 0..10 {
        let y0 = 7.0 + 2.5 * i as f64;
        prisms.push(Prism {
            section: vec![(0.7, 11.3), (19.3, 11.3), (19.3, 11.8), (0.7, 11.8)],
            y: (y0, y0 + 0.5),
            material: rib.clone(),
        });
    }
    let room = Room::box_with_prisms(
        v(0.0, 0.0, 0.0),
        v(20.0, 32.0, 12.0),
        [
            walls.clone(),
            walls.clone(),
            walls.clone(),
            walls,
            seats,
            ceiling,
        ],
        &prisms,
    )?;
    Ok(Space {
        room,
        air: Air::standard(),
        sources: sources(v(10.0, 3.2, 2.3), v(12.5, 3.2, 2.3), v(7.5, 3.2, 2.3)),
        positions: positions(v(9.2, 12.0, 1.2), v(12.0, 28.0, 1.2)),
        non_diffuse: false,
        seam_hz: None,
        max_time_s: 4.0,
        occupancy: "unoccupied upholstered seats, empty stage",
        reference: gated(
            Decay::T30,
            octaves([
                (1.1, 1.5),
                (1.4, 1.7),
                (1.7, 1.7),
                (1.6, 1.7),
                (1.6, 1.7),
                (1.4, 1.5),
            ]),
            "DAGA Table 2: two halls of 7,700–8,000 m³ with audience simulation",
        ),
        notes: vec![
            "a raised stage and two side galleries as solids",
            "pilasters and ceiling ribs as solids; coffers, doors and window recesses still by depth (0.5–1.5 m)",
        ],
    })
}

pub fn shoebox_hall() -> Result<Space, Error> {
    let walls = surface(
        "hall walls",
        &[
            part(m::PLASTER, 0.55, 1.2),
            part(m::WOOD_LINING, 0.35, 0.6),
            part(m::GLASS, 0.1, 0.3),
        ],
    )?;
    let seats = surface(
        "audience floor",
        &[part(m::SEATS, 0.85, 0.45), part(m::WOOD_FLOOR, 0.15, 0.0)],
    )?;
    let ceiling = surface("hall ceiling", &[part(m::PLASTER, 1.0, 1.2)])?;
    let stage = surface("stage", &[part(m::WOOD_FLOOR, 1.0, 0.0)])?;
    let balcony = surface(
        "side balconies",
        &[part(m::SEATS, 0.6, 0.45), part(m::PLASTER, 0.4, 0.6)],
    )?;
    // The long axis is y. A raised stage and two side balconies are solids (see the recital hall).
    let pilaster = surface("pilasters", &[part(m::PLASTER, 1.0, 0.0)])?;
    let rib = surface("ceiling ribs", &[part(m::PLASTER, 1.0, 0.0)])?;
    let mut prisms = vec![
        Prism {
            section: vec![(1.0, 0.0), (23.0, 0.0), (23.0, 1.0), (1.0, 1.0)],
            y: (0.5, 10.0),
            material: stage,
        },
        Prism {
            section: vec![(0.5, 6.0), (3.5, 6.0), (3.5, 7.0), (0.5, 7.0)],
            y: (11.0, 45.0),
            material: balcony.clone(),
        },
        Prism {
            section: vec![(20.5, 6.0), (23.5, 6.0), (23.5, 7.0), (20.5, 7.0)],
            y: (11.0, 45.0),
            material: balcony,
        },
    ];
    // Fifteen pilasters a side above the balconies and ten ceiling ribs, as solids.
    for i in 0..15 {
        let y0 = 11.5 + 2.2 * i as f64;
        for x in [0.08, 23.22] {
            prisms.push(Prism {
                section: vec![(x, 7.4), (x + 0.7, 7.4), (x + 0.7, 13.0), (x, 13.0)],
                y: (y0, y0 + 0.7),
                material: pilaster.clone(),
            });
        }
    }
    for i in 0..10 {
        let y0 = 11.5 + 3.4 * i as f64;
        prisms.push(Prism {
            section: vec![(0.8, 15.2), (23.2, 15.2), (23.2, 15.7), (0.8, 15.7)],
            y: (y0, y0 + 0.5),
            material: rib.clone(),
        });
    }
    let room = Room::box_with_prisms(
        v(0.0, 0.0, 0.0),
        v(24.0, 46.0, 16.0),
        [
            walls.clone(),
            walls.clone(),
            walls.clone(),
            walls,
            seats,
            ceiling,
        ],
        &prisms,
    )?;
    Ok(Space {
        room,
        air: Air::standard(),
        sources: sources(v(12.0, 5.0, 2.5), v(15.0, 5.0, 2.5), v(9.0, 5.0, 2.5)),
        positions: positions(v(13.0, 16.0, 1.2), v(10.5, 39.0, 1.2)),
        non_diffuse: false,
        seam_hz: None,
        max_time_s: 7.0,
        occupancy: "unoccupied upholstered seats, empty stage",
        reference: gated(
            Decay::T30,
            octaves([
                (2.0, 3.2),
                (2.0, 3.2),
                (2.1, 2.8),
                (2.0, 2.9),
                (1.9, 2.8),
                (1.7, 2.3),
            ]),
            "DAGA Table 2: seven shoebox halls with audience simulation",
        ),
        notes: vec![
            "a raised stage and two side balconies as solids",
            "pilasters and ceiling ribs as solids; statues and coffers still by depth (0.6–1.2 m)",
        ],
    })
}

pub fn scoring_stage() -> Result<Space, Error> {
    let walls = surface(
        "walls",
        &[
            relief(m::WOOD_LINING, 0.3, 0.8),
            relief(m::PLASTER, 0.4, 0.8),
            part(m::VELOUR, 0.3, 0.2),
        ],
    )?;
    let floor = surface("floor", &[part(m::WOOD_FLOOR, 1.0, 0.0)])?;
    let ceiling = surface(
        "ceiling",
        &[
            relief(m::PLASTER, 0.5, 0.8),
            part(m::GLASS_WOOL, 0.2, 0.0),
            relief(m::WOOD_LINING, 0.3, 0.8),
        ],
    )?;
    Ok(Space {
        room: Room::shoebox(v(33.0, 22.0, 10.0), box_faces(&walls, floor, ceiling))?,
        air: Air::standard(),
        sources: sources(v(10.0, 11.0, 1.4), v(10.0, 7.0, 1.4), v(10.0, 15.0, 1.4)),
        positions: positions(v(15.0, 10.6, 3.0), v(27.0, 12.2, 4.0)),
        non_diffuse: false,
        seam_hz: None,
        max_time_s: 5.0,
        occupancy: "empty stage, curtains over 30 % of the walls",
        reference: reported(
            Decay::T30,
            mid((1.0, 2.2)),
            "Murphy 2013: three stages, mid-band values in prose",
        ),
        notes: vec![
            "the near position is a tree above the conductor, the far a pair of room microphones",
        ],
    })
}

pub fn horseshoe_opera() -> Result<Space, Error> {
    let stage = [
        surface("stage floor", &[part(m::WOOD_FLOOR, 1.0, 0.0)])?,
        surface(
            "fly tower",
            &[part(m::BRICK, 0.5, 0.0), part(m::VELOUR, 0.5, 1.0)],
        )?,
        surface(
            "stage walls",
            &[
                part(m::WOOD_LINING, 0.4, 0.0),
                part(m::BRICK, 0.3, 0.0),
                part(m::VELOUR, 0.3, 0.5),
            ],
        )?,
    ];
    let arch = [
        surface("pit", &[part(m::WOOD_FLOOR, 1.0, 0.0)])?,
        surface("proscenium soffit", &[part(m::PLASTER, 1.0, 0.0)])?,
        surface("proscenium", &[part(m::PLASTER, 1.0, 0.0)])?,
    ];
    let hall = [
        surface(
            "stalls",
            &[part(m::SEATS, 0.7, 0.45), part(m::WOOD_FLOOR, 0.3, 0.0)],
        )?,
        surface(
            "auditorium ceiling",
            &[
                relief(m::WOOD_LINING, 0.5, 0.2),
                relief(m::PLASTER, 0.5, 0.2),
            ],
        )?,
        surface(
            "tiers of boxes",
            &[
                part(m::VELOUR, 0.35, 1.0),
                relief(m::WOOD_LINING, 0.65, 1.0),
            ],
        )?,
    ];
    let [stage_floor, stage_ceiling, stage_walls] = stage;
    let [arch_floor, arch_ceiling, arch_walls] = arch;
    let [hall_floor, hall_ceiling, hall_walls] = hall;
    let room = Room::stepped_regions(&[
        Region {
            plan: vec![
                (0.0, 0.0),
                (14.0, 0.0),
                (14.0, 7.0),
                (14.0, 17.0),
                (14.0, 24.0),
                (0.0, 24.0),
            ],
            materials: [stage_floor, stage_ceiling, stage_walls],
            height: 26.0,
        },
        Region {
            plan: vec![(14.0, 7.0), (15.0, 7.0), (15.0, 17.0), (14.0, 17.0)],
            materials: [arch_floor, arch_ceiling, arch_walls],
            height: 10.0,
        },
        Region {
            plan: vec![
                (15.0, 1.0),
                (28.0, 1.0),
                (34.0, 3.5),
                (37.5, 8.0),
                (38.0, 12.0),
                (37.5, 16.0),
                (34.0, 20.5),
                (28.0, 23.0),
                (15.0, 23.0),
                (15.0, 17.0),
                (15.0, 7.0),
            ],
            materials: [hall_floor, hall_ceiling, hall_walls],
            height: 17.0,
        },
    ])?;
    Ok(Space {
        room,
        air: Air::standard(),
        sources: sources(v(11.0, 12.0, 1.7), v(11.0, 9.0, 1.7), v(11.0, 15.0, 1.7)),
        positions: positions(v(22.0, 11.2, 1.2), v(35.0, 13.0, 1.2)),
        non_diffuse: false,
        seam_hz: None,
        max_time_s: 5.0,
        occupancy: "unoccupied, scenery flown, iron curtain open",
        reference: reported(
            Decay::T20,
            octaves([
                (1.79, 1.79),
                (1.61, 1.61),
                (1.37, 1.37),
                (1.24, 1.24),
                (1.18, 1.18),
                (1.11, 1.11),
            ]),
            "Krauss Table 2: one horseshoe house, iron curtain open",
        ),
        notes: vec![
            "the stage house is a taller coupled volume behind a 10 m proscenium",
            "the tiers of boxes are a blend on the horseshoe wall, not geometry",
        ],
    })
}

/// The recital hall class: DAGA Table 2's two halls.
fn recital_range() -> Target {
    octaves([
        (1.1, 1.5),
        (1.4, 1.7),
        (1.7, 1.7),
        (1.6, 1.7),
        (1.6, 1.7),
        (1.4, 1.5),
    ])
}

/// The shoebox concert hall class: DAGA Table 2's seven halls.
fn shoebox_range() -> Target {
    octaves([
        (2.0, 3.2),
        (2.0, 3.2),
        (2.1, 2.8),
        (2.0, 2.9),
        (1.9, 2.8),
        (1.7, 2.3),
    ])
}

fn audience_floor() -> Result<Material, Error> {
    surface(
        "audience floor",
        &[part(m::SEATS, 0.85, 0.45), part(m::WOOD_FLOOR, 0.15, 0.0)],
    )
}

/// A hall whose long axis is y: a raised stage `(depth, height)` from y = 0.5, side balconies
/// `(width, level, material)`, and `extra` solids of the room's own (pilasters, ribs). The balconies
/// run from behind the stage to the rear; all of it is geometry, as in the recital hall.
/// Materials `[walls, audience floor, ceiling, stage]`.
fn hall(
    size: Vec3,
    (depth, height): (f64, f64),
    balconies: Option<(f64, f64, Material)>,
    [walls, floor, ceiling, stage]: [Material; 4],
    extra: Vec<Prism>,
) -> Result<Room, Error> {
    let width = size.x;
    let mut prisms = vec![Prism {
        section: vec![
            (1.0, 0.0),
            (width - 1.0, 0.0),
            (width - 1.0, height),
            (1.0, height),
        ],
        y: (0.5, depth),
        material: stage,
    }];
    if let Some((span, level, material)) = balconies {
        for x in [0.5, width - 0.5 - span] {
            prisms.push(Prism {
                section: vec![
                    (x, level),
                    (x + span, level),
                    (x + span, level + 1.0),
                    (x, level + 1.0),
                ],
                y: (depth + 1.0, size.y - 1.0),
                material: material.clone(),
            });
        }
    }
    prisms.extend(extra);
    let faces = [
        walls.clone(),
        walls.clone(),
        walls.clone(),
        walls,
        floor,
        ceiling,
    ];
    Room::box_with_prisms(v(0.0, 0.0, 0.0), size, faces, &prisms)
}

/// Sources on a [`hall`]'s stage and receivers in its stalls. Listeners face −y, so their left is +x.
fn hall_layout(size: Vec3, (depth, height): (f64, f64)) -> (Sources, [Position; 2]) {
    let (w, l) = (size.x, size.y);
    let centre = v(0.5 * w, 0.55 * depth, height + 1.5);
    let spread = v(0.125 * w, 0.0, 0.0);
    (
        sources(centre, centre + spread, centre - spread),
        positions(v(0.46 * w, 0.375 * l, 1.2), v(0.6 * w, 0.875 * l, 1.2)),
    )
}

pub fn chamber_music_hall() -> Result<Space, Error> {
    let (size, stage) = (v(14.0, 24.0, 9.0), (5.0, 0.7));
    let walls = surface(
        "hall walls",
        &[
            relief(m::WOOD_LINING, 0.6, 0.6),
            relief(m::PLASTER, 0.3, 0.6),
            relief(m::GLASS, 0.1, 0.3),
        ],
    )?;
    let ceiling = surface(
        "hall ceiling",
        &[
            relief(m::PLASTERBOARD, 0.5, 1.0),
            relief(m::WOOD_LINING, 0.5, 1.0),
        ],
    )?;
    let stage_floor = surface("stage", &[part(m::WOOD_FLOOR, 1.0, 0.0)])?;
    let pilaster = surface("pilasters", &[part(m::PLASTER, 1.0, 0.0)])?;
    let rib = surface("ceiling ribs", &[part(m::WOOD_LINING, 1.0, 0.0)])?;
    let mut prisms = vec![Prism {
        section: vec![(1.0, 0.0), (13.0, 0.0), (13.0, 0.7), (1.0, 0.7)],
        y: (0.5, 0.5 + stage.0),
        material: stage_floor,
    }];
    for i in 0..16 {
        let y0 = 6.5 + 1.044 * i as f64;
        for x in [0.08, 13.42] {
            prisms.push(Prism {
                section: vec![(x, 0.35), (x + 0.5, 0.35), (x + 0.5, 6.6), (x, 6.6)],
                y: (y0, y0 + 0.55),
                material: pilaster.clone(),
            });
        }
    }
    for i in 0..10 {
        let y0 = 6.5 + 1.778 * i as f64;
        prisms.push(Prism {
            section: vec![(0.7, 8.35), (13.3, 8.35), (13.3, 8.85), (0.7, 8.85)],
            y: (y0, y0 + 0.45),
            material: rib.clone(),
        });
    }
    let room = Room::box_with_prisms(
        v(0.0, 0.0, 0.0),
        size,
        [
            walls.clone(),
            walls.clone(),
            walls.clone(),
            walls,
            audience_floor()?,
            ceiling,
        ],
        &prisms,
    )?;
    Ok(space(
        room,
        hall_layout(size, stage),
        3.5,
        "unoccupied upholstered seats, empty stage",
        reported(
            Decay::T30,
            recital_range(),
            "DAGA Table 2: two recital halls of 7,700-8,000 m3, a larger class",
        ),
        vec![
            "a raised stage, thirty-two wall pilasters and ten ceiling ribs as solids; coffers and recesses still by depth",
        ],
    ))
}

pub fn small_concert_hall() -> Result<Space, Error> {
    let (size, stage) = (v(18.0, 30.0, 11.0), (7.0, 0.9));
    let walls = surface(
        "hall walls",
        &[
            part(m::PLASTER, 0.5, 1.0),
            part(m::WOOD_LINING, 0.4, 0.8),
            part(m::GLASS, 0.1, 0.3),
        ],
    )?;
    let ceiling = surface(
        "hall ceiling",
        &[part(m::PLASTER, 0.6, 1.2), part(m::WOOD_LINING, 0.4, 1.2)],
    )?;
    let stage_floor = surface("stage", &[part(m::WOOD_FLOOR, 1.0, 0.0)])?;
    let balcony = surface(
        "side balconies",
        &[part(m::SEATS, 0.5, 0.45), part(m::PLASTER, 0.5, 0.6)],
    )?;
    let pilaster = surface("pilasters", &[part(m::PLASTER, 1.0, 0.0)])?;
    let rib = surface("ceiling ribs", &[part(m::PLASTER, 1.0, 0.0)])?;
    let solids = pilasters_and_ribs(
        size,
        Axis::Y,
        (8.5, 29.0),
        12,
        (0.6, 0.55, (6.4, 9.5)),
        (8, (10.0, 10.45), 0.5),
        &pilaster,
        &rib,
    );
    Ok(space(
        hall(
            size,
            stage,
            Some((2.5, 5.0, balcony)),
            [walls, audience_floor()?, ceiling, stage_floor],
            solids,
        )?,
        hall_layout(size, stage),
        4.0,
        "unoccupied upholstered seats, empty stage",
        reported(
            Decay::T30,
            recital_range(),
            "DAGA Table 2: two recital halls of 7,700–8,000 m³, a neighbour class",
        ),
        vec![
            "a raised stage, two side balconies, twenty-four pilasters and eight ceiling ribs as solids; coffers still by depth (0.8–1.2 m)",
        ],
    ))
}

pub fn large_shoebox_hall() -> Result<Space, Error> {
    let (size, stage) = (v(26.0, 50.0, 18.0), (11.0, 1.1));
    let walls = surface(
        "hall walls",
        &[
            part(m::PLASTER, 0.6, 1.2),
            part(m::WOOD_LINING, 0.3, 0.6),
            part(m::GLASS, 0.1, 0.3),
        ],
    )?;
    let ceiling = surface("hall ceiling", &[part(m::PLASTER, 1.0, 1.5)])?;
    let stage_floor = surface("stage", &[part(m::WOOD_FLOOR, 1.0, 0.0)])?;
    let balcony = surface(
        "side balconies",
        &[part(m::SEATS, 0.6, 0.45), part(m::PLASTER, 0.4, 0.6)],
    )?;
    let pilaster = surface("pilasters", &[part(m::PLASTER, 1.0, 0.0)])?;
    let rib = surface("ceiling ribs", &[part(m::PLASTER, 1.0, 0.0)])?;
    let solids = pilasters_and_ribs(
        size,
        Axis::Y,
        (12.5, 48.0),
        16,
        (0.7, 0.6, (8.4, 15.0)),
        (10, (16.9, 17.4), 0.5),
        &pilaster,
        &rib,
    );
    Ok(space(
        hall(
            size,
            stage,
            Some((3.5, 7.0, balcony)),
            [walls, audience_floor()?, ceiling, stage_floor],
            solids,
        )?,
        hall_layout(size, stage),
        7.0,
        "unoccupied upholstered seats, empty stage",
        reported(
            Decay::T30,
            shoebox_range(),
            "DAGA Table 2: seven shoebox halls of 10,000–18,780 m³, a smaller class",
        ),
        vec![
            "a raised stage, two side balconies, thirty-two pilasters and ten ceiling ribs as solids; statues and coffers still by depth (0.6–1.5 m)",
        ],
    ))
}

pub fn vineyard_hall() -> Result<Space, Error> {
    let walls = surface(
        "hall walls",
        &[
            part(m::WOOD_LINING, 0.4, 1.2),
            part(m::PLASTER, 0.5, 1.2),
            part(m::GLASS, 0.1, 0.3),
        ],
    )?;
    let floor = surface(
        "stalls",
        &[part(m::SEATS, 0.7, 0.45), part(m::WOOD_FLOOR, 0.3, 0.0)],
    )?;
    let ceiling = surface(
        "ceiling and canopy",
        &[part(m::PLASTER, 0.6, 2.0), part(m::WOOD_LINING, 0.4, 2.0)],
    )?;
    let stage = surface("stage", &[part(m::WOOD_FLOOR, 1.0, 0.0)])?;
    let terrace = surface(
        "seating terraces",
        &[part(m::SEATS, 0.7, 0.45), part(m::WOOD_LINING, 0.3, 0.8)],
    )?;
    // The stage stands in the room; terraces rise around it: a choir block behind, raked blocks on
    // both sides, a block at the rear.
    let block = |section: Vec<(f64, f64)>, y: (f64, f64), material: &Material| Prism {
        section,
        y,
        material: material.clone(),
    };
    let prisms = [
        block(
            vec![(12.0, 0.0), (24.0, 0.0), (24.0, 1.0), (12.0, 1.0)],
            (8.0, 18.0),
            &stage,
        ),
        block(
            vec![(10.0, 0.0), (26.0, 0.0), (26.0, 1.5), (10.0, 1.5)],
            (1.0, 5.0),
            &terrace,
        ),
        block(
            vec![(1.0, 0.0), (8.0, 0.0), (8.0, 1.2), (1.0, 3.0)],
            (6.0, 34.0),
            &terrace,
        ),
        block(
            vec![(28.0, 0.0), (35.0, 0.0), (35.0, 3.0), (28.0, 1.2)],
            (6.0, 34.0),
            &terrace,
        ),
        block(
            vec![(10.0, 0.0), (26.0, 0.0), (26.0, 2.0), (10.0, 2.0)],
            (26.0, 38.0),
            &terrace,
        ),
    ];
    let faces = [
        walls.clone(),
        walls.clone(),
        walls.clone(),
        walls,
        floor,
        ceiling,
    ];
    Ok(space(
        Room::box_with_prisms(v(0.0, 0.0, 0.0), v(36.0, 40.0, 17.0), faces, &prisms)?,
        (
            sources(v(18.0, 13.0, 2.5), v(21.0, 13.0, 2.5), v(15.0, 13.0, 2.5)),
            positions(v(17.2, 22.0, 1.2), v(19.5, 33.0, 3.2)),
        ),
        5.0,
        "unoccupied upholstered seats, empty stage",
        reported(
            Decay::T30,
            mid((1.56, 3.28)),
            "Neal–Vigeant §4.1: unoccupied halls of every shape, hall-average mid-band T30 over 23 configurations",
        ),
        vec![
            "the stage stands in the room with seating terraces as solids around it; the far receiver sits on the rear terrace",
            "terrace fronts and the canopy by characteristic depth (0.8–2.0 m)",
        ],
    ))
}

pub fn fan_auditorium() -> Result<Space, Error> {
    let floor = surface(
        "seating",
        &[part(m::SEATS, 0.8, 0.45), part(m::WOOD_FLOOR, 0.2, 0.0)],
    )?;
    let ceiling = surface(
        "ceiling",
        &[
            relief(m::PLASTERBOARD, 0.6, 0.8),
            relief(m::WOOD_LINING, 0.4, 0.8),
        ],
    )?;
    let walls = surface(
        "walls",
        &[
            relief(m::PLASTER, 0.3, 0.5),
            relief(m::WOOD_LINING, 0.4, 0.5),
            part(m::VELOUR, 0.3, 0.2),
        ],
    )?;
    let plan = [(0.0, 10.0), (30.0, 0.0), (30.0, 34.0), (0.0, 24.0)];
    Ok(space(
        under_sloping_ceiling(&plan, (8.0, 0.13, 0.0), [floor, ceiling, walls])?,
        (
            sources(v(4.0, 17.0, 1.5), v(4.0, 14.5, 1.5), v(4.0, 19.5, 1.5)),
            positions(v(12.0, 16.2, 1.2), v(26.0, 18.5, 1.2)),
        ),
        3.0,
        "unoccupied upholstered seats",
        reported(
            Decay::T30,
            mid((0.8, 1.2)),
            "survey row 14: cinema and lecture-theatre range, 0.8–1.2 s (a design guide)",
        ),
        vec![
            "a fan plan widening from the stage under a ceiling rising from 8 m to 12 m; stage at the narrow end",
        ],
    ))
}

pub fn cinema() -> Result<Space, Error> {
    let front = surface("front floor", &[part(m::CARPET, 1.0, 0.01)])?;
    let seats = surface(
        "raked seating",
        &[part(m::SEATS, 0.9, 0.4), part(m::CARPET, 0.1, 0.01)],
    )?;
    let walls = surface(
        "walls",
        &[
            part(m::VELOUR, 0.35, 0.1),
            part(m::GLASS_WOOL, 0.15, 0.0),
            part(m::PLASTERBOARD, 0.50, 0.0),
        ],
    )?;
    let ceiling = surface(
        "ceiling",
        &[
            part(m::GLASS_WOOL, 0.2, 0.0),
            part(m::PLASTERBOARD, 0.8, 0.0),
        ],
    )?;
    let screen = surface(
        "screen wall",
        &[part(m::GLASS_WOOL, 0.6, 0.0), part(m::HARD, 0.4, 0.0)],
    )?;
    // Cross-section in (x, z), swept across y: the screen at x = 0, a flat front, a rake rising
    // 4.5 m to the back wall.
    let section = [
        ((0.0, 0.0), 0),
        ((8.0, 0.0), 1),
        ((26.0, 4.5), 2),
        ((26.0, 10.0), 3),
        ((0.0, 10.0), 4),
    ];
    Ok(space(
        swept(
            Axis::Y,
            &section,
            18.0,
            2,
            vec![front, seats, walls, ceiling, screen],
        )?,
        (
            sources(v(1.0, 9.0, 4.0), v(1.0, 4.0, 4.0), v(1.0, 14.0, 4.0)),
            positions(v(13.0, 8.4, 2.45), v(23.0, 9.8, 4.95)),
        ),
        2.0,
        "unoccupied seats",
        reported(
            Decay::T30,
            mid((0.8, 1.2)),
            "survey row 14: cinema and lecture-theatre range, 0.8–1.2 s (a design guide)",
        ),
        vec![
            "the sources stand for the loudspeakers behind the screen; the rows of the rake by depth (0.4 m)",
        ],
    ))
}

pub fn drama_theatre() -> Result<Space, Error> {
    let region = |plan: Vec<(f64, f64)>, materials: [Material; 3], height: f64| Region {
        plan,
        materials,
        height,
    };
    let stage = [
        surface("stage floor", &[part(m::WOOD_FLOOR, 1.0, 0.0)])?,
        surface(
            "fly tower",
            &[part(m::BRICK, 0.5, 0.0), part(m::VELOUR, 0.5, 1.0)],
        )?,
        surface(
            "stage walls",
            &[
                part(m::WOOD_LINING, 0.4, 0.0),
                part(m::BRICK, 0.3, 0.0),
                part(m::VELOUR, 0.3, 0.5),
            ],
        )?,
    ];
    let arch = [
        surface("forestage", &[part(m::WOOD_FLOOR, 1.0, 0.0)])?,
        surface("proscenium soffit", &[part(m::PLASTER, 1.0, 0.0)])?,
        surface("proscenium", &[part(m::PLASTER, 1.0, 0.0)])?,
    ];
    let house = [
        surface(
            "stalls",
            &[part(m::SEATS, 0.75, 0.45), part(m::WOOD_FLOOR, 0.25, 0.0)],
        )?,
        surface(
            "auditorium ceiling",
            &[
                relief(m::PLASTERBOARD, 0.6, 0.5),
                relief(m::WOOD_LINING, 0.4, 0.5),
            ],
        )?,
        surface(
            "auditorium walls",
            &[
                relief(m::WOOD_LINING, 0.45, 0.5),
                part(m::VELOUR, 0.30, 0.1),
                relief(m::PLASTER, 0.25, 0.5),
            ],
        )?,
    ];
    let room = Room::stepped_regions(&[
        region(
            vec![
                (0.0, 0.0),
                (12.0, 0.0),
                (12.0, 5.0),
                (12.0, 15.0),
                (12.0, 20.0),
                (0.0, 20.0),
            ],
            stage,
            20.0,
        ),
        region(
            vec![(12.0, 5.0), (13.0, 5.0), (13.0, 15.0), (12.0, 15.0)],
            arch,
            8.0,
        ),
        region(
            vec![
                (13.0, 1.0),
                (32.0, 1.0),
                (32.0, 19.0),
                (13.0, 19.0),
                (13.0, 15.0),
                (13.0, 5.0),
            ],
            house,
            11.0,
        ),
    ])?;
    Ok(space(
        room,
        (
            sources(v(9.5, 10.0, 1.7), v(9.5, 7.5, 1.7), v(9.5, 12.5, 1.7)),
            positions(v(18.0, 9.4, 1.2), v(29.5, 10.8, 1.2)),
        ),
        3.0,
        "unoccupied, scenery flown",
        reported(
            Decay::T30,
            mid((0.9, 1.3)),
            "estimate: a drama theatre, drier than the opera houses' 1.26–2.15 s mid-band (research notes §12)",
        ),
        vec![
            "a stage house 20 m high behind a 10 m proscenium, a rectangular auditorium 11 m high",
        ],
    ))
}

pub fn lecture_theatre() -> Result<Space, Error> {
    let front = surface("front floor", &[part(m::WOOD_FLOOR, 1.0, 0.0)])?;
    let benches = surface(
        "raked benches",
        &[
            part(m::MEDIUM_SEATS, 0.5, 0.5),
            part(m::PEWS, 0.3, 0.5),
            part(m::WOOD_FLOOR, 0.2, 0.0),
        ],
    )?;
    let walls = surface(
        "walls",
        &[
            part(m::PLASTER, 0.4, 0.3),
            part(m::PERFORATED_WOOD, 0.2, 0.05),
            part(m::WOOD_LINING, 0.3, 0.3),
            part(m::GLASS, 0.1, 0.0),
        ],
    )?;
    let ceiling = surface(
        "ceiling",
        &[
            part(m::PLASTERBOARD, 0.55, 0.0),
            part(m::MINERAL_BOARD, 0.45, 0.0),
        ],
    )?;
    let board = surface(
        "front wall",
        &[part(m::WOOD_LINING, 0.5, 0.0), part(m::HARD, 0.5, 0.0)],
    )?;
    let section = [
        ((0.0, 0.0), 0),
        ((4.0, 0.0), 1),
        ((16.0, 3.6), 2),
        ((16.0, 7.5), 3),
        ((0.0, 6.5), 4),
    ];
    Ok(space(
        swept(
            Axis::Y,
            &section,
            14.0,
            2,
            vec![front, benches, walls, ceiling, board],
        )?,
        (
            sources(v(1.5, 7.0, 1.6), v(1.5, 4.5, 1.6), v(1.5, 9.5, 1.6)),
            positions(v(8.0, 6.5, 2.4), v(14.0, 7.6, 4.2)),
        ),
        2.5,
        "unoccupied seats",
        reported(
            Decay::T30,
            mid((0.8, 1.2)),
            "survey row 14: cinema and lecture-theatre range, 0.8–1.2 s (a design guide)",
        ),
        vec![
            "tip-up seats and desks on a rake rising 3.6 m, medium upholstered seats and the pews row 0.5 m deep; a board wall at the front; perforated wood panels on a fifth of the walls, pilasters by depth (0.3 m)",
        ],
    ))
}

pub fn church_scoring_stage() -> Result<Space, Error> {
    let floor = surface("floor", &[part(m::WOOD_FLOOR, 1.0, 0.0)])?;
    let walls = surface(
        "walls",
        &[
            relief(m::PLASTER, 0.5, 0.8),
            relief(m::BRICK, 0.3, 0.6),
            relief(m::LEADED_GLAZING, 0.1, 0.3),
            part(m::VELOUR, 0.1, 0.5),
        ],
    )?;
    let roof = surface(
        "open roof",
        &[
            relief(m::WOOD_LINING, 0.6, 1.0),
            relief(m::PLASTER, 0.4, 1.0),
        ],
    )?;
    let section = [
        ((0.0, 0.0), 0),
        ((20.0, 0.0), 1),
        ((20.0, 13.0), 2),
        ((10.0, 19.0), 2),
        ((0.0, 13.0), 1),
    ];
    Ok(space(
        swept(Axis::X, &section, 38.0, 1, vec![floor, walls, roof])?,
        (
            sources(v(10.0, 10.0, 1.4), v(10.0, 6.5, 1.4), v(10.0, 13.5, 1.4)),
            positions(v(16.0, 9.6, 3.0), v(31.0, 10.6, 4.0)),
        ),
        7.0,
        "empty, curtains over a tenth of the walls",
        reported(
            Decay::T30,
            mid((3.0, 7.5)),
            "survey row 8 [23], a trade report: a scoring stage in a converted church, variable 3–7.5 s",
        ),
        vec![
            "a gabled nave 38 m long under an open timber roof",
            "the near position is a tree above the conductor, the far a pair of room microphones",
        ],
    ))
}

pub fn small_scoring_stage() -> Result<Space, Error> {
    let size = v(24.0, 16.0, 8.5);
    let walls = surface(
        "walls",
        &[
            relief(m::WOOD_LINING, 0.25, 0.8),
            relief(m::PLASTER, 0.30, 0.8),
            part(m::VELOUR, 0.45, 0.2),
        ],
    )?;
    let floor = surface("floor", &[part(m::WOOD_FLOOR, 1.0, 0.0)])?;
    let ceiling = surface(
        "ceiling",
        &[
            relief(m::PLASTER, 0.50, 0.8),
            part(m::GLASS_WOOL, 0.35, 0.0),
            relief(m::WOOD_LINING, 0.15, 0.8),
        ],
    )?;
    Ok(space(
        boxed(size, &walls, floor, ceiling)?,
        layout(size),
        3.5,
        "empty stage, curtains over 45 % of the walls",
        reported(
            Decay::T30,
            mid((1.0, 1.3)),
            "Murphy 2013: a converted shooting stage of 4,530 m³, 1.0–1.3 s mid-band in prose",
        ),
        vec!["irregular walls and ceiling by characteristic depth (0.8 m), movable curtains"],
    ))
}
