# Bundled assets

`north-america.json` is generated from Natural Earth's public-domain 110m country polygons and state/province boundaries. It contains geographic points and boundary lines. It requires no runtime download.

Sources are [countries](https://github.com/nvkelso/natural-earth-vector/blob/master/geojson/ne_110m_admin_0_countries.geojson) and [states](https://github.com/nvkelso/natural-earth-vector/blob/master/geojson/ne_110m_admin_1_states_provinces_lines.geojson). Natural Earth's [terms](https://www.naturalearthdata.com/about/terms-of-use/) place the data in the public domain.

`world.json` comes from the same country polygons. It holds simplified coastline rings and a land mask on a 0.5° grid, stored as one hexadecimal row per latitude band. The Nearby view and slates outside North America use it.

To regenerate both files, download those GeoJSON files and run `python3 scripts/build_map.py <countries.geojson> <states.geojson>`.

`demo.json` contains illustrative schedules, scores, hits, and errors for the demo mode. Dates before the demo day read as final and later dates as not yet played. They are not live results. The app labels this mode on screen. Recorded responses in `tests/fixtures` exercise the real MLB data parser separately.
