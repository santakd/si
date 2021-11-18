import {
  BehaviorSubject,
  combineLatest,
  debounceTime,
  from,
  shareReplay,
} from "rxjs";
import { changeSet$ } from "@/observable/change_set";
import { editSession$ } from "@/observable/edit_session";
import { switchMap } from "rxjs/operators";
import {
  NO_CHANGE_SET_PK,
  NO_EDIT_SESSION_PK,
  Visibility,
} from "@/api/sdf/dal/visibility";

export const showDeleted$ = new BehaviorSubject<boolean>(false);

export const visibility$ = combineLatest([
  changeSet$,
  editSession$,
  showDeleted$,
]).pipe(
  debounceTime(10),
  switchMap(([changeSet, editSession, showDeleted]) => {
    const visibility_change_set_pk = changeSet?.pk || NO_CHANGE_SET_PK;
    const visibility_edit_session_pk = editSession?.pk || NO_EDIT_SESSION_PK;
    const visibility_deleted = showDeleted;
    const visibility: Visibility = {
      visibility_change_set_pk,
      visibility_edit_session_pk,
      visibility_deleted,
    };
    return from([visibility]);
  }),
  shareReplay(1),
);
