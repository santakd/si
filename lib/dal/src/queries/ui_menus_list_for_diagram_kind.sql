SELECT schema_ui_menus.id                       AS id,
       schema_ui_menus.visibility_change_set_pk AS visibility_change_set_pk,
       row_to_json(schema_ui_menus.*)           AS object

FROM schema_ui_menus
WHERE schema_ui_menus.diagram_kind = $3
  AND in_tenancy_and_visible_v1($1, $2, schema_ui_menus);