use super::*;

pub fn echo_chamber() -> Result<Space, Error> {
    let walls = surface(
        "walls",
        &[part(m::HARD, 0.97, 0.0), part(m::WOOD_LINING, 0.03, 0.0)],
    )?;
    let floor = surface("floor", &[part(m::CONCRETE_FLOOR, 1.0, 0.0)])?;
    let ceiling = surface("ceiling", &[part(m::HARD, 1.0, 0.0)])?;
    let plan = [(0.0, 0.0), (5.2, 0.3), (4.8, 4.1), (0.3, 3.7)];
    Ok(Space {
        room: under_sloping_ceiling(&plan, (2.8, 0.06, 0.04), [floor, ceiling, walls])?,
        air: Air::standard(),
        sources: sources(v(1.0, 1.2, 1.4), v(1.28, 0.78, 1.4), v(0.72, 1.62, 1.4)),
        positions: positions(v(2.8, 2.4, 1.6), v(4.0, 3.2, 2.0)),
        non_diffuse: false,
        seam_hz: Some(250.0),
        max_time_s: 7.5,
        occupancy: "empty chamber, a loudspeaker and microphones",
        reference: reported(
            Decay::T30,
            mid((3.0, 5.0)),
            "estimate (research notes §7): press and vendor statements only",
        ),
        notes: vec![
            "no two surfaces parallel: splayed walls under a sloping ceiling",
            "seam chosen at 250 Hz, below the Schroeder frequency (about 470 Hz): at 400 Hz the wave solver would take half an hour per source",
            "the wood lining row stands for the door",
        ],
    })
}

/// A chamber of splayed walls under a sloping ceiling, in one hard finish but for its door, over a
/// concrete floor.
fn splayed_chamber(
    plan: &[(f64, f64)],
    ceiling_plane: (f64, f64, f64),
    finish: m::Row,
) -> Result<Room, Error> {
    let walls = surface(
        "walls",
        &[part(finish, 0.97, 0.0), part(m::WOOD_LINING, 0.03, 0.0)],
    )?;
    let floor = surface("floor", &[part(m::CONCRETE_FLOOR, 1.0, 0.0)])?;
    let ceiling = surface("ceiling", &[part(finish, 1.0, 0.0)])?;
    under_sloping_ceiling(plan, ceiling_plane, [floor, ceiling, walls])
}

pub fn small_echo_chamber() -> Result<Space, Error> {
    let plan = [(0.0, 0.0), (3.8, 0.2), (3.5, 3.0), (0.2, 2.7)];
    Ok(Space {
        seam_hz: Some(250.0),
        ..space(
            splayed_chamber(&plan, (2.3, 0.05, 0.035), m::HARD)?,
            (
                sources(v(0.8, 1.1, 1.3), v(0.8, 0.7, 1.3), v(0.8, 1.5, 1.3)),
                positions(v(2.0, 1.6, 1.4), v(2.9, 2.1, 1.7)),
            ),
            5.0,
            "empty chamber, a loudspeaker and microphones",
            reported(
                Decay::T30,
                mid((1.5, 3.0)),
                "estimate: smaller than the survey's 57 m³ chamber at 3–3.5 s (survey row 9)",
            ),
            vec![
                "no two surfaces parallel: splayed walls under a sloping ceiling",
                "seam chosen at 250 Hz, below the Schroeder frequency, for cost",
                "the wood lining row stands for the door",
            ],
        )
    })
}

pub fn large_echo_chamber() -> Result<Space, Error> {
    let plan = [(0.0, 0.0), (8.2, 0.4), (7.8, 6.8), (0.3, 6.2)];
    Ok(Space {
        seam_hz: Some(150.0),
        ..space(
            splayed_chamber(&plan, (3.4, 0.08, 0.05), m::HARD)?,
            (
                sources(v(1.4, 3.0, 1.5), v(1.4, 2.2, 1.5), v(1.4, 3.8, 1.5)),
                positions(v(4.2, 2.7, 1.6), v(6.8, 4.1, 2.2)),
            ),
            9.0,
            "empty chamber, a loudspeaker and microphones",
            reported(
                Decay::T30,
                mid((3.5, 5.5)),
                "estimate: larger than the survey's 57 m³ chamber at 3–3.5 s, up to 5 s (survey row 9)",
            ),
            vec![
                "no two surfaces parallel: splayed walls under a sloping ceiling",
                "seam chosen at 150 Hz, below the Schroeder frequency, for cost",
                "the wood lining row stands for the door",
            ],
        )
    })
}

pub fn tiled_chamber() -> Result<Space, Error> {
    let plan = [(0.0, 0.0), (4.6, 0.3), (4.3, 3.6), (0.2, 3.3)];
    Ok(Space {
        seam_hz: Some(200.0),
        ..space(
            splayed_chamber(&plan, (2.6, 0.05, 0.04), m::TILE)?,
            (
                sources(v(1.0, 1.6, 1.4), v(1.0, 1.1, 1.4), v(1.0, 2.1, 1.4)),
                positions(v(2.5, 1.5, 1.5), v(3.6, 2.3, 1.8)),
            ),
            8.0,
            "empty chamber, a loudspeaker and microphones",
            reported(
                Decay::T30,
                mid((4.0, 7.0)),
                "estimate: a glazed chamber, longer and brighter than the survey's concrete one at 3–3.5 s (survey row 9)",
            ),
            vec![
                "glazed tile walls and ceiling over a concrete floor; no two surfaces parallel",
                "seam chosen at 200 Hz, below the Schroeder frequency, for cost",
            ],
        )
    })
}

pub fn water_tank() -> Result<Space, Error> {
    const RADIUS: f64 = 4.0;
    let plan: Vec<(f64, f64)> = (0..16)
        .map(|k| {
            let a = 2.0 * PI * k as f64 / 16.0;
            (RADIUS + RADIUS * a.cos(), RADIUS + RADIUS * a.sin())
        })
        .collect();
    let floor = surface(
        "floor",
        &[part(m::WATER, 0.8, 0.0), part(m::HARD, 0.2, 0.05)],
    )?;
    let concrete = surface("concrete", &[part(m::HARD, 1.0, 0.0)])?;
    Ok(Space {
        seam_hz: Some(120.0),
        ..space(
            Room::extruded(&plan, 5.5, [floor, concrete.clone(), concrete])?,
            (
                sources(v(1.8, 4.0, 1.6), v(1.8, 3.2, 1.6), v(1.8, 4.8, 1.6)),
                positions(v(3.8, 3.8, 1.6), v(6.2, 4.5, 3.8)),
            ),
            17.0,
            "drained to a shallow water floor",
            reported(
                Decay::T30,
                mid((6.0, 10.0)),
                "estimate: a concrete tank, no published measurement (survey row 25)",
            ),
            vec![
                "a sixteen-sided concrete cylinder, 8 m across and 5.5 m high, under a flat roof",
                "seam chosen at 120 Hz, below the Schroeder frequency, for cost",
                "the render ends at 10 s with a 0.5 s fade; metrics come from the uncapped render",
            ],
        )
    })
}

pub fn cistern() -> Result<Space, Error> {
    let size = v(18.0, 10.0, 5.0);
    let walls = surface(
        "walls",
        &[part(m::PLASTER, 0.6, 0.6), part(m::BRICK, 0.4, 0.6)],
    )?;
    let floor = surface(
        "floor",
        &[part(m::WATER, 0.7, 0.0), part(m::SANDSTONE, 0.3, 0.0)],
    )?;
    let ceiling = surface(
        "cross vaults",
        &[part(m::BRICK, 0.5, 1.0), part(m::PLASTER, 0.5, 1.0)],
    )?;
    Ok(space(
        boxed(size, &walls, floor, ceiling)?,
        layout(size),
        9.0,
        "a shallow pool of water",
        reported(
            Decay::T30,
            mid((3.0, 6.0)),
            "estimate: a rendered brick cistern, no published measurement (survey row 25)",
        ),
        vec![
            "columns and cross vaults by characteristic depth (0.6–1.0 m), not as geometry",
            "walls in hydraulic render, the smooth plaster row, and exposed brick",
        ],
    ))
}

pub fn grain_silo() -> Result<Space, Error> {
    const RADIUS: f64 = 3.0;
    let plan: Vec<(f64, f64)> = (0..16)
        .map(|k| {
            let a = 2.0 * PI * k as f64 / 16.0;
            (RADIUS + RADIUS * a.cos(), RADIUS + RADIUS * a.sin())
        })
        .collect();
    let floor = surface("floor", &[part(m::CONCRETE_FLOOR, 1.0, 0.0)])?;
    let concrete = surface("concrete", &[part(m::HARD, 1.0, 0.0)])?;
    Ok(Space {
        seam_hz: Some(100.0),
        ..space(
            Room::extruded(&plan, 22.0, [floor, concrete.clone(), concrete])?,
            (
                sources(v(1.4, 3.0, 1.6), v(1.4, 2.4, 1.6), v(1.4, 3.6, 1.6)),
                positions(v(3.2, 2.8, 1.6), v(4.0, 3.5, 12.0)),
            ),
            17.0,
            "empty",
            reported(
                Decay::T30,
                mid((5.0, 9.0)),
                "estimate: an empty concrete silo, no published measurement",
            ),
            vec![
                "a sixteen-sided concrete cylinder, 6 m across and 22 m high",
                "near: at the foot, beside the source; far: a microphone hung 12 m up",
                "seam chosen at 100 Hz, below the Schroeder frequency, for cost",
                "the render ends at 10 s with a 0.5 s fade; metrics come from the uncapped render",
            ],
        )
    })
}

pub fn rock_cave() -> Result<Space, Error> {
    let floor = surface(
        "rubble floor",
        &[part(m::SANDSTONE, 0.6, 0.3), part(m::BALLAST, 0.4, 0.2)],
    )?;
    let ceiling = surface(
        "rock roof",
        &[
            part(m::SANDSTONE, 0.8, 1.5),
            part(m::BLOCK_COARSE, 0.2, 0.6),
        ],
    )?;
    let walls = surface(
        "rock walls",
        &[
            part(m::SANDSTONE, 0.55, 1.2),
            part(m::BLOCK_COARSE, 0.45, 0.6),
        ],
    )?;
    let plan = [
        (0.0, 2.0),
        (4.0, 0.0),
        (11.0, 0.5),
        (15.0, 3.0),
        (16.0, 8.0),
        (12.0, 11.5),
        (6.0, 12.0),
        (1.5, 10.0),
        (0.0, 6.0),
    ];
    let boulder = surface(
        "boulders",
        &[
            part(m::SANDSTONE, 0.7, 0.6),
            part(m::BLOCK_COARSE, 0.3, 0.4),
        ],
    )?;
    let wedge = |x: (f64, f64), z: (f64, f64), y: (f64, f64)| Prism {
        section: vec![(x.0, z.0), (x.1, z.0), (0.5 * (x.0 + x.1), z.1)],
        y,
        material: boulder.clone(),
    };
    let slab = |x: (f64, f64), z: (f64, f64), y: (f64, f64)| Prism {
        section: vec![(x.0, z.1), (x.1, z.1), (0.5 * (x.0 + x.1), z.0)],
        y,
        material: boulder.clone(),
    };
    let solids = [
        wedge((2.6, 4.4), (0.25, 1.9), (2.6, 4.0)),
        wedge((4.6, 6.2), (0.25, 1.4), (8.4, 10.0)),
        wedge((9.0, 10.8), (0.25, 2.1), (2.2, 3.6)),
        wedge((11.2, 13.0), (0.25, 1.6), (6.4, 8.2)),
        wedge((8.2, 9.6), (0.25, 1.2), (9.0, 10.4)),
        slab((3.4, 6.0), (2.6, 3.9), (5.6, 7.0)),
        slab((9.4, 12.0), (2.4, 3.6), (4.2, 5.4)),
    ];
    Ok(space(
        under_sloping_ceiling(&plan, (4.5, 0.12, 0.08), [floor, ceiling, walls])?
            .with_solids(&solids)?,
        (
            sources(v(3.5, 6.0, 1.6), v(3.5, 4.8, 1.6), v(3.5, 7.2, 1.6)),
            positions(v(7.0, 5.6, 1.6), v(12.5, 7.2, 1.8)),
        ),
        3.0,
        "empty",
        reported(
            Decay::T30,
            mid((1.2, 1.2)),
            "survey row 25: one limestone chamber, 1.2 s",
        ),
        vec![
            "a chamber of nine rock walls under a tilted roof, with five boulders and two hanging slabs as solids at angles; the walls keep their relief by depth (0.6–1.5 m)",
            "a chamber of nine rock walls under a tilted roof, its relief by characteristic depth (0.6–1.5 m)",
            "no row for rock: rough sandstone and coarse unpainted block stand in, scree the ballast row",
        ],
    ))
}

pub fn catacomb() -> Result<Space, Error> {
    const HEIGHT: f64 = 2.3;
    let materials = || -> Result<[Material; 3], Error> {
        Ok([
            surface("floor", &[part(m::SANDSTONE, 1.0, 0.0)])?,
            surface("roof", &[part(m::SANDSTONE, 1.0, 0.3)])?,
            surface(
                "gallery walls",
                &[part(m::SANDSTONE, 0.6, 0.5), part(m::BRICK, 0.4, 0.5)],
            )?,
        ])
    };
    // A crossing 2.2 m square and four galleries off it, meeting vertex to vertex.
    let regions = [
        vec![(14.0, 10.0), (16.2, 10.0), (16.2, 12.2), (14.0, 12.2)],
        vec![(16.2, 10.0), (32.0, 10.0), (32.0, 12.2), (16.2, 12.2)],
        vec![(6.0, 10.0), (14.0, 10.0), (14.0, 12.2), (6.0, 12.2)],
        vec![(14.0, 2.0), (16.2, 2.0), (16.2, 10.0), (14.0, 10.0)],
        vec![(14.0, 12.2), (16.2, 12.2), (16.2, 20.0), (14.0, 20.0)],
    ];
    let regions = regions
        .into_iter()
        .map(|plan| Ok((plan, materials()?)))
        .collect::<Result<Vec<_>, Error>>()?;
    Ok(Space {
        non_diffuse: true,
        ..space(
            Room::extruded_regions(&regions, HEIGHT)?,
            (
                sources(v(15.1, 11.1, 1.5), v(15.1, 10.5, 1.5), v(15.1, 11.7, 1.5)),
                positions(v(20.0, 10.9, 1.5), v(30.5, 11.4, 1.5)),
            ),
            3.5,
            "empty",
            reported(
                Decay::T30,
                mid((1.0, 2.0)),
                "estimate: no published measurement (survey row 25)",
            ),
            vec![
                "four galleries 2.2 m wide off a crossing; the sources at the crossing, the receivers down one gallery, which sees them",
                "burial niches by characteristic depth (0.5 m)",
                "long galleries keep their flutter discrete, so the scene is non-diffuse",
            ],
        )
    })
}
