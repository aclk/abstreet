initSidebarItems({"fn":[["match_points_to_lanes","Snap points to an exact Position along the nearest lane. If the result doesn’t contain a requested point, then there was no matching lane close enough."],["trim_path","Adjust the path to start on the polygon’s border, not center."]],"mod":[["bridges",""],["buildings",""],["collapse_intersections",""],["initial","Naming is confusing, but RawMap -> InitialMap -> Map. InitialMap is separate pretty much just for the step of producing https://a-b-street.github.io/docs/tech/map/importing/geometry.html."],["medians",""],["merge_intersections",""],["parking_lots",""],["remove_disconnected",""],["snappy",""],["traffic_signals","The various traffic signal generators live in the traffic signal module. Eventually, we might want to move to a trait. For now, there’s a single make_traffic_signal static method in each generator file, which is called to generate a traffic signal of a particular flavor."],["transit",""],["turns",""],["walking_turns",""]],"struct":[["RawToMapOptions","Options for converting RawMaps to Maps."]]});