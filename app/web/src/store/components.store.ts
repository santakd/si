import { defineStore } from "pinia";
import _ from "lodash";
import { Vector2d } from "konva/lib/types";
import { ApiRequest } from "@/utils/pinia_api_tools";

import { addStoreHooks } from "@/utils/pinia_hooks_plugin";
import {
  DiagramContent,
  DiagramEdgeDef,
  DiagramNodeDef,
  DiagramStatusIcon,
} from "@/organisms/GenericDiagram/diagram_types";
import { MenuItem } from "@/api/sdf/dal/menu";
import {
  DiagramNode,
  DiagramSchemaVariant,
  DiagramSchemaVariants,
} from "@/api/sdf/dal/diagram";
import { ComponentStats, ComponentStatus } from "@/api/sdf/dal/change_set";
import { LabelList } from "@/api/sdf/dal/label_list";
import {
  ComponentDiff,
  ComponentIdentification,
} from "@/api/sdf/dal/component";
import { Resource } from "@/api/sdf/dal/resource";
import { CodeView } from "@/api/sdf/dal/code_view";
import { ChangeSetId, useChangeSetsStore } from "./change_sets.store";
import { useRealtimeStore } from "./realtime/realtime.store";
import {
  QualificationStatus,
  useQualificationsStore,
} from "./qualifications.store";
import { useWorkspacesStore } from "./workspaces.store";

export type ComponentId = number;
type Component = {
  id: ComponentId;
  displayName: string;
  schemaName: string;
  schemaId: number;
  schemaVariantId: number;
  schemaVariantName: string;
  color: string;
  changeStatus?: ComponentStatus;
  // TODO: probably want to move this to a different store and not load it all the time
  resource: Resource;
};

type SocketId = number;

type SchemaId = number;

type NodeAddMenu = {
  displayName: string;
  schemas: {
    id: SchemaId;
    displayName: string;
    color: string;
  }[];
}[];

const qualificationStatusToIconMap: Record<
  QualificationStatus,
  DiagramStatusIcon
> = {
  success: { icon: "check", tone: "success" },
  failure: { icon: "alert", tone: "error" },
  running: { icon: "loading", tone: "info" },
};

export const useComponentsStore = (forceChangeSetId?: ChangeSetId) => {
  const workspacesStore = useWorkspacesStore();
  const workspaceId = workspacesStore.selectedWorkspaceId;

  // this needs some work... but we'll probably want a way to force using HEAD
  // so we can load HEAD data in some scenarios while also loading a change set?
  let changeSetId: ChangeSetId | null;
  if (forceChangeSetId) {
    changeSetId = forceChangeSetId;
  } else {
    const changeSetsStore = useChangeSetsStore();
    changeSetId = changeSetsStore.selectedChangeSetId;
  }

  // TODO: probably these should be passed in automatically
  // and need to make sure it's done consistently (right now some endpoints vary slightly)
  const visibilityParams = {
    visibility_change_set_pk: changeSetId,
    workspaceId,
  };

  return addStoreHooks(
    defineStore(`cs${changeSetId}/components`, {
      state: () => ({
        // components within this changeset
        // componentsById: {} as Record<ComponentId, Component>,
        // connectionsById: {} as Record<ConnectionId, Connection>,
        // added / deleted / modified
        componentIdentificationsById: {} as Record<
          ComponentId,
          ComponentIdentification
        >,
        componentChangeStatusById: {} as Record<ComponentId, ComponentStatus>,

        componentCodeViewsById: {} as Record<ComponentId, CodeView[]>,
        componentDiffsById: {} as Record<ComponentId, ComponentDiff>,

        rawDiagramNodes: [] as DiagramNodeDef[],
        diagramEdges: [] as DiagramEdgeDef[],
        schemaVariantsById: {} as Record<SchemaId, DiagramSchemaVariant>,
        rawNodeAddMenu: [] as MenuItem[],

        selectedComponentId: null as ComponentId | null,

        /** number of operations being executed on component. Basically a semaphore `loading` state  */
        activityCounterByComponentId: {} as Record<ComponentId, number>,
      }),
      getters: {
        // transforming the diagram-y data back into more generic looking data
        // TODO: ideally we just fetch it like this...
        componentsById(): Record<ComponentId, Component> {
          const diagramNodesById = _.keyBy(this.rawDiagramNodes, (n) => n.id);
          return _.mapValues(this.componentIdentificationsById, (ci) => {
            const diagramNode = diagramNodesById[ci.componentId];
            return {
              id: ci.componentId,
              displayName: diagramNode?.subtitle,
              schemaId: ci.schemaId,
              schemaName: ci.schemaName,
              schemaVariantId: ci.schemaVariantId,
              schemaVariantName: ci.schemaVariantName,
              // TODO: probably want to move this into its own store
              resource: ci.resource,
              color: diagramNode?.color,
              changeStatus: this.componentChangeStatusById[ci.componentId],
            } as Component;
          });
        },
        allComponents(): Component[] {
          return _.values(this.componentsById);
        },

        selectedComponent(): Component {
          return this.componentsById[this.selectedComponentId || 0];
        },
        selectedComponentDiff(): ComponentDiff | undefined {
          return this.componentDiffsById[this.selectedComponentId || 0];
        },
        selectedComponentCode(): CodeView[] | undefined {
          return this.componentCodeViewsById[this.selectedComponentId || 0];
        },

        diagramNodes(): DiagramNodeDef[] {
          // adding logo and qualification info into the nodes
          // TODO: probably want to include logo directly
          return _.map(this.rawDiagramNodes, (node) => {
            // Default to "si" if we do not have a logo.
            let typeIcon = "si";
            if (
              node.category === "AWS" ||
              node.category === "CoreOS" ||
              node.category === "Docker" ||
              node.category === "Kubernetes"
            ) {
              typeIcon = node.category;
            }

            const qualificationsStore = useQualificationsStore();
            const qualificationStatus =
              qualificationsStore.qualificationStatusByComponentId[
                parseInt(node.id)
              ];

            const activityCounter = _.get(
              this.activityCounterByComponentId,
              node.id,
              0,
            );

            return {
              ...node,
              isLoading: activityCounter > 0,
              typeIcon,
              statusIcons: _.compact([
                qualificationStatusToIconMap[qualificationStatus],
              ]),
            };
          });
        },
        // allConnections: (state) => _.values(state.connectionsById),

        schemaVariants: (state) => _.values(state.schemaVariantsById),

        nodeAddMenu(): NodeAddMenu {
          return _.compact(
            _.map(this.rawNodeAddMenu, (category) => {
              // all root level items are categories for now... will probably rework this endpoint anyway
              if (category.kind !== "category") return null;
              return {
                displayName: category.name,
                // TODO: add color + logo on categories?
                schemas: _.compact(
                  _.map(category.items, (item) => {
                    // ignoring "link" items - don't think these are relevant at the moment
                    if (item.kind !== "item") return;

                    // TODO: return hex code from backend...
                    const schemaVariant =
                      this.schemaVariantsById[item.schema_id];
                    const colorInt = schemaVariant?.color;
                    const color = colorInt
                      ? `#${colorInt.toString(16)}`
                      : "#777";

                    return {
                      displayName: item.name,
                      id: item.schema_id,
                      // links: item.links, // not sure this is needed?
                      color,
                    };
                  }),
                ),
              };
            }),
          );
        },

        changeStatsSummary(): Record<ComponentStatus | "total", number> {
          const allChanged = _.filter(
            this.allComponents,
            (c) => !!c.changeStatus,
          );
          const grouped = _.groupBy(allChanged, (c) => c.changeStatus);
          return {
            added: grouped.added?.length || 0,
            deleted: grouped.deleted?.length || 0,
            modified: grouped.modified?.length || 0,
            total: allChanged.length,
          };
        },
      },
      actions: {
        // TODO: change these endpoints to return a more complete picture of component data in one call
        // see also component/get_components_metadata endpoint which was not used anymore but has some more data we may want to include

        // actually fetches diagram-style data, but we have a computed getter to turn back into more generic component data above
        async FETCH_DIAGRAM_DATA() {
          return new ApiRequest<DiagramContent>({
            url: "diagram/get_diagram",
            params: {
              ...visibilityParams,
            },
            onSuccess: (response) => {
              // for now just storing the diagram-y data
              // but I think ideally we fetch more generic component data and then transform into diagram format as necessary
              this.rawDiagramNodes = response.nodes;
              this.diagramEdges = response.edges;
            },
          });
        },
        // fetches a dropdown-style list of some component data, also including resources?
        async FETCH_COMPONENTS() {
          return new ApiRequest<{ list: LabelList<ComponentIdentification> }>({
            url: "component/list_components_identification",
            params: {
              ...visibilityParams,
            },
            onSuccess: (response) => {
              // endpoint returns dropdown-y data
              const rawIdentifications = _.map(response.list, "value");
              this.componentIdentificationsById = _.keyBy(
                rawIdentifications,
                (c) => c.componentId,
              );
            },
          });
        },

        // used when adding new nodes
        async FETCH_AVAILABLE_SCHEMAS() {
          return new ApiRequest<DiagramSchemaVariants>({
            // TODO: probably switch to something like GET `/workspaces/:id/schemas`?
            url: "diagram/list_schema_variants",
            params: {
              ...visibilityParams,
            },
            onSuccess: (response) => {
              this.schemaVariantsById = _.keyBy(response, "id");
            },
          });
        },

        async FETCH_NODE_ADD_MENU() {
          return new ApiRequest<MenuItem[]>({
            method: "post",
            // TODO: probably combine into single call with FETCH_AVAILABLE_SCHEMAS
            url: "diagram/get_node_add_menu",
            params: {
              ...visibilityParams,
            },
            onSuccess: (response) => {
              this.rawNodeAddMenu = response;
            },
          });
        },

        async FETCH_CHANGE_STATS() {
          return new ApiRequest<{ componentStats: ComponentStats }>({
            url: "change_set/get_stats",
            params: {
              ...visibilityParams,
            },
            onSuccess: (response) => {
              this.componentChangeStatusById = _.transform(
                response.componentStats.stats,
                (acc, cs) => {
                  acc[cs.componentId] = cs.componentStatus;
                },
                {} as Record<ComponentId, ComponentStatus>,
              );
            },
          });
        },

        async SET_COMPONENT_DIAGRAM_POSITION(
          componentId: ComponentId,
          position: Vector2d,
        ) {
          return new ApiRequest<{ componentStats: ComponentStats }>({
            method: "post",
            url: "diagram/set_node_position",
            params: {
              nodeId: componentId,
              x: position.x.toString(),
              y: position.y.toString(),
              diagramKind: "configuration",
              ...visibilityParams,
            },
            onSuccess: (response) => {
              // record position change rather than wait for re-fetch
            },
          });
        },
        async CREATE_COMPONENT(schemaId: number, position: Vector2d) {
          return new ApiRequest<{ node: DiagramNode }>({
            method: "post",
            url: "diagram/create_node",
            params: {
              schemaId,
              x: position.x.toString(),
              y: position.y.toString(),
              ...visibilityParams,
            },
            onSuccess: (response) => {
              // TODO: store component details rather than waiting for re-fetch
            },
          });
        },
        async CREATE_COMPONENT_CONNECTION(
          from: { componentId: ComponentId; socketId: SocketId },
          to: { componentId: ComponentId; socketId: SocketId },
        ) {
          return new ApiRequest<{ node: DiagramNode }>({
            method: "post",
            url: "diagram/create_connection",
            params: {
              fromNodeId: from.componentId,
              fromSocketId: from.socketId,
              toNodeId: to.componentId,
              toSocketId: to.socketId,
              ...visibilityParams,
            },
            onSuccess: (response) => {
              // TODO: store component details rather than waiting for re-fetch
            },
          });
        },

        async TRIGGER_COMPONENT_CODE_GEN(componentId: ComponentId) {
          return new ApiRequest<{ success: true }>({
            method: "post",
            url: "component/generate_code",
            keyRequestStatusBy: componentId,
            params: {
              componentId,
              ...visibilityParams,
            },
            // no onSuccess here - we just wait for websocket
          });
        },
        async FETCH_COMPONENT_CODE(componentId: ComponentId) {
          return new ApiRequest<{ codeViews: CodeView[] }>({
            url: "component/get_code",
            keyRequestStatusBy: componentId,
            params: {
              componentId,
              ...visibilityParams,
            },
            onSuccess: (response) => {
              this.componentCodeViewsById[componentId] = response.codeViews;
            },
          });
        },

        async FETCH_COMPONENT_DIFF(componentId: ComponentId) {
          return new ApiRequest<{ componentDiff: ComponentDiff }>({
            url: "func/get_diff",
            keyRequestStatusBy: componentId,
            params: {
              componentId,
              ...visibilityParams,
            },
            onSuccess: (response) => {
              this.componentDiffsById[componentId] = response.componentDiff;
            },
          });
        },

        setSelectedComponentId(id: ComponentId | null) {
          if (!id) this.selectedComponentId = null;
          else {
            if (this.componentsById[id]) {
              this.selectedComponentId = id;
            } else {
              // TODO: not sure... do we throw an error? Do we select the id anyway?
              this.selectedComponentId = null;
            }
          }
        },

        increaseActivityCounterOnComponent(id: ComponentId) {
          const activityCounter = this.activityCounterByComponentId;

          activityCounter[id] = activityCounter[id] || 0;
          activityCounter[id] += 1;
        },

        decreaseActivityCounterOnComponent(id: ComponentId) {
          const activityCounter = this.activityCounterByComponentId;

          if (_.isNil(activityCounter[id]) || activityCounter[id] <= 0)
            throw new Error(
              `Trying to decrease activityCounter on component(id: ${id}) that didn't have any activities`,
            );

          activityCounter[id] -= 1;
        },
      },
      onActivated() {
        this.FETCH_DIAGRAM_DATA();
        this.FETCH_COMPONENTS();
        this.FETCH_AVAILABLE_SCHEMAS();
        this.FETCH_NODE_ADD_MENU();
        this.FETCH_CHANGE_STATS();

        const realtimeStore = useRealtimeStore();

        realtimeStore.subscribe(this.$id, `changeset/${changeSetId}`, [
          {
            eventType: "ChangeSetWritten",
            callback: (writtenChangeSetId) => {
              // ideally we wouldn't have to check this - since the topic subscription
              // would mean we only receive the event for this changeset already...
              // but this is fine for now
              if (writtenChangeSetId !== changeSetId) return;

              // probably want to get pushed updates instead of blindly re-fetching, but this is the first step of getting things working
              this.FETCH_DIAGRAM_DATA();
              this.FETCH_COMPONENTS();
              this.FETCH_CHANGE_STATS();
            },
          },
          {
            eventType: "CodeGenerated",
            callback: (codeGeneratedEvent) => {
              // probably ideally just push the new code over the websocket
              // but for now we'll re-fetch if the component is currently selected
              // topic subscription would also help to know if we're talking about the component in the correct changeset
              if (this.selectedComponentId === codeGeneratedEvent.componentId) {
                this.FETCH_COMPONENT_CODE(this.selectedComponentId);
              }
            },
          },
        ]);

        return () => {
          realtimeStore.unsubscribe(this.$id);
        };
      },
    }),
  )();
};