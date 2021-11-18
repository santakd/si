export interface WsEvent {
  version: number;
  billing_account_ids: Array<number>;
  payload: WsPayload;
}

export interface WsChangeSetCreated {
  kind: "ChangeSetCreated";
  data: number;
}
export interface WsChangeSetApplied {
  kind: "ChangeSetApplied";
  data: number;
}
export interface WsChangeSetCanceled {
  kind: "ChangeSetCanceled";
  data: number;
}
export interface WsSchemaCreated {
  kind: "SchemaCreated";
  data: number;
}

export type WsPayload =
  | WsSchemaCreated
  | WsChangeSetCreated
  | WsChangeSetApplied
  | WsChangeSetCanceled;
