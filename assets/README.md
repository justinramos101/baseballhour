# Bundled assets

`north-america.json` is generated from Natural Earth's public-domain 110m country polygons and state/province boundaries. It contains geographic points and boundary lines. It requires no runtime download.

Sources are [countries](https://github.com/nvkelso/natural-earth-vector/blob/master/geojson/ne_110m_admin_0_countries.geojson) and [states](https://github.com/nvkelso/natural-earth-vector/blob/master/geojson/ne_110m_admin_1_states_provinces_lines.geojson). Natural Earth's [terms](https://www.naturalearthdata.com/about/terms-of-use/) place the data in the public domain.

To regenerate, download those GeoJSON files and run `python3 scripts/build_map.py <countries.geojson> <states.geojson>`.

`demo.json` contains illustrative schedules and scores for the demo mode. They are not live results. The app labels this mode on screen. Recorded responses in `tests/fixtures` exercise the real MLB data parser separately.
