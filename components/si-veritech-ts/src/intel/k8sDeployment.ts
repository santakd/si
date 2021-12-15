import Debug from "debug";
const debug = Debug("veritech:controllers:intel:k8sDeployment");
import {
  baseCheckQualifications,
  baseRunCommands,
  baseSyncResource,
  forEachCluster,
} from "./k8sShared";
import {
  InferPropertiesReply,
  InferPropertiesRequest,
} from "../controllers/inferProperties";
import {
  SetArrayEntryFromAllEntities,
  setArrayEntryFromAllEntities,
  setProperty,
  setPropertyFromEntity,
  setPropertyFromProperty,
} from "./inferShared";
import { findEntityByType } from "../support";
import {
  CommandProtocolFinish,
  SyncResourceRequest,
} from "../controllers/syncResource";
import WebSocket from "ws";
import { ResourceInternalHealth } from "si-entity";
import { SiCtx } from "../siCtx";

export function inferProperties(
  request: InferPropertiesRequest,
): InferPropertiesReply {
  const context = request.context;
  const entity = request.entity;

  setProperty({
    entity,
    toPath: ["metadata", "name"],
    value: entity.name,
  });

  setPropertyFromProperty({
    entity,
    fromPath: ["metadata", "name"],
    toPath: ["metadata", "labels", "app"],
  });

  setPropertyFromProperty({
    entity,
    fromPath: ["metadata", "labels", "app"],
    toPath: ["spec", "selector", "matchLabels", "app"],
  });

  setPropertyFromProperty({
    entity,
    fromPath: ["metadata", "labels", "app"],
    toPath: ["spec", "template", "metadata", "labels", "app"],
  });

  // Do you have a k8s namespace? If so, set the namespace.
  setPropertyFromEntity({
    context,
    entityType: "k8sNamespace",
    fromPath: ["metadata", "name"],
    toEntity: entity,
    toPath: ["metadata", "namespace"],
  });

  // The template should have a namespace that matches the namespace of the
  // object we are deploying.
  setPropertyFromProperty({
    entity,
    fromPath: ["metadata", "namespace"],
    toPath: ["spec", "template", "metadata", "namespace"],
  });

  /*
   Leaving the next block of code because we might need it when we automate the creation of an
   imagePullSecrets on edge connection.
  */

  // setArrayEntryFromAllEntities({
  //   entity,
  //   context,
  //   entityType: "k8sSecret",
  //   toPath: ["spec", "template", "spec", "imagePullSecrets"],
  //   valuesCallback(
  //     fromEntry,
  //   ): ReturnType<SetArrayEntryFromAllEntities["valuesCallback"]> {
  //     const toSet: { path: string[]; value: any; system: string }[] = [];

  //     const secretValues = fromEntry.entity.getPropertyForAllSystems({
  //       path: ["metadata", "name"],
  //     });
  //     if (secretValues) {
  //       for (const system in secretValues) {
  //         if (secretValues[system]) {
  //           toSet.push({
  //             path: ["name"],
  //             value: secretValues[system],
  //             system,
  //           });
  //         }
  //       }
  //     }

  //     return toSet;
  //   },
  // });

  // NOTE(fnichol): commenting out k8sConfigMap->k8sDeployment temporarily
  // per story
  // https://app.clubhouse.io/systeminit/story/1516/don-t-create-a-volume-automatically-when-connecting-a-k8sconfigmap-to-a-k8sdeployment
  //
  // setArrayEntryFromAllEntities({
  //   entity,
  //   context,
  //   entityType: "k8sConfigMap",
  //   toPath: ["spec", "template", "spec", "volumes"],
  //   valuesCallback(
  //     fromEntry,
  //   ): ReturnType<SetArrayEntryFromAllEntities["valuesCallback"]> {
  //     const toSet: { path: string[]; value: any; system: string }[] = [];

  //     const configMapValues = fromEntry.entity.getPropertyForAllSystems({
  //       path: ["metadata", "name"],
  //     });
  //     if (configMapValues) {
  //       for (const system in configMapValues) {
  //         if (configMapValues[system]) {
  //           toSet.push({
  //             path: ["name"],
  //             value: configMapValues[system],
  //             system,
  //           });
  //           toSet.push({
  //             path: ["configMap", "name"],
  //             value: configMapValues[system],
  //             system,
  //           });
  //         }
  //       }
  //     }
  //     return toSet;
  //   },
  // });

  // volumeMounts:
  // - name: config
  // mountPath: /etc/otel

  setArrayEntryFromAllEntities({
    entity,
    context,
    entityType: "dockerImage",
    toPath: ["spec", "template", "spec", "containers"],
    valuesCallback(
      fromEntry,
    ): ReturnType<SetArrayEntryFromAllEntities["valuesCallback"]> {
      const toSet: { path: string[]; value: any; system: string }[] = [];
      toSet.push({
        path: ["name"],
        value: fromEntry.entity.name,
        system: "baseline",
      });
      const imageValues = fromEntry.entity.getPropertyForAllSystems({
        path: ["image"],
      });
      for (const system in imageValues) {
        toSet.push({ path: ["image"], value: imageValues[system], system });
        const checkForTag = new RegExp("^.+(:.+)$");
        const checkResult = checkForTag.exec(imageValues[system] as string);
        if (checkResult && checkResult[1] == ":latest") {
          toSet.push({ path: ["imagePullPolicy"], value: "Always", system });
        } else if (checkResult) {
          toSet.push({
            path: ["imagePullPolicy"],
            value: "IfNotPresent",
            system,
          });
        } else {
          toSet.push({ path: ["imagePullPolicy"], value: "Always", system });
        }
      }
      const exposedPortValues = fromEntry.entity.getPropertyForAllSystems({
        path: ["ExposedPorts"],
      });
      for (const system in exposedPortValues) {
        const exposedPortList: string[] = exposedPortValues[system] as string[];
        for (const exposedPortValue of exposedPortList) {
          const exposedPortParts: string[] = exposedPortValue.split("/");
          const portNumber = exposedPortParts[0];
          const portProtocol = exposedPortParts[1]
            ? exposedPortParts[1].toUpperCase()
            : "TCP";
          toSet.push({
            path: ["ports"],
            value: {
              name: `port-${portNumber}`,
              containerPort: portNumber,
              protocol: portProtocol,
            },
            system,
          });
        }
      }

      const volumeMountsValues = entity.getPropertyForAllSystems({
        path: ["spec", "template", "spec", "volumes"],
      });

      if (volumeMountsValues) {
        for (const system in volumeMountsValues) {
          if (volumeMountsValues[system]) {
            toSet.push({
              path: ["volumeMounts", "name"],
              value: volumeMountsValues[system],
              system,
            });
          }
        }
      }

      return toSet;
    },
  });

  // if contaienrs exists
  // setProperty({
  //   entity,
  //   toPath: ["metadata", "name"],
  //   value: entity.name,
  // });

  return { entity };
}

export async function syncResource(
  ctx: typeof SiCtx,
  req: SyncResourceRequest,
  ws: WebSocket,
): Promise<CommandProtocolFinish["finish"]> {
  const response = await baseSyncResource(ctx, req, ws);
  const nameSpace = findEntityByType(req, "k8sNamespace");
  const system = req.system.id;
  const defaultArgs = ["rollout", "status", "deployment", "-w", "--timeout=5s"];
  if (nameSpace) {
    defaultArgs.push("-n");
    defaultArgs.push(
      nameSpace.getProperty({ system, path: ["metadata", "name"] }),
    );
  }

  await forEachCluster(
    ctx,
    req,
    ws,
    async (_kubeYaml, kubeConfigDir, execEnv, kubeCluster) => {
      const result = await ctx.exec(
        "kubectl",
        [
          ...defaultArgs,
          "--kubeconfig",
          `${kubeConfigDir.path}/config`,
          req.entity.getProperty({ system, path: ["metadata", "name"] }),
        ],
        { env: execEnv, reject: false },
      );
      if (result.exitCode != 0) {
        response.health = "error";
        response.state = "error";
        response.internalHealth = ResourceInternalHealth.Error;
        response.error = result.all;
        if (response.subResources[kubeCluster.id]) {
          // @ts-ignore
          response.subResources[kubeCluster.id].state = "error";
          // @ts-ignore
          response.subResources[kubeCluster.id].health = "error";
          // @ts-ignore
          response.subResources[kubeCluster.id].internalHealth =
            ResourceInternalHealth.Error;
          // @ts-ignore
          response.subResources[kubeCluster.id].error = result.all;
        }
      }
    },
  );
  response.data = response.subResources;
  return response;
}

export default {
  inferProperties,
  checkQualifications: baseCheckQualifications,
  runCommands: baseRunCommands,
  syncResource,
};