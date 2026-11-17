#! /bin/bash

ogr2ogr -f GeoJSON -clipdst 70 0 140 55 china_land_hd.geojson land_polygons.shp
