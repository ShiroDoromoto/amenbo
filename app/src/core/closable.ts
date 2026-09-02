// Pure front-end readings of a `role: closable` dimension (`AMB-D-829`). `closed` on a value is the
// payload of that role, the way a period is the time axis's — the flag sits on every value, and what
// the role decides is whether closing one is offered at all.
//
// Closing retires a value from what a record is newly filed under and takes nothing away: the records
// already on it keep it, and a filter naming it goes on resolving. Which is why each face reads the
// flag to its own end — the filter offers a closed value, the picker hides it unless the record
// carries it, the board draws its column while cards remain in it, and this panel shows every value.
import type { DimensionDto } from "../bindings/bindings";

/** Can this dimension's values be closed? Only a closable axis can — reopening is free on any axis. */
export function isClosable(dim: DimensionDto): boolean {
  return dim.role === "closable";
}
