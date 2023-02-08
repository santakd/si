SELECT row_to_json(n.*) AS object
FROM nodes n
WHERE n.id IN (SELECT id
               FROM nodes
               WHERE visibility_change_set_pk = ident_nil_v1()
                 AND visibility_deleted_at IS NULL
                 AND in_tenancy_v1($1, tenancy_workspace_pk))

  AND visibility_change_set_pk = $2
  AND visibility_deleted_at IS NOT NULL

  AND in_tenancy_v1($1, tenancy_workspace_pk)
ORDER BY n.id DESC

