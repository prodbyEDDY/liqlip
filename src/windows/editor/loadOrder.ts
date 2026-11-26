// Ordering of the editor window's loads (ARCHITECTURE §18.9). Two things can race:
// - two loads (the first `shot_editor_get`, then a `shot:editor` while the first picture is still
//   decoding): the NEWEST started load wins — an older one finishing later must not put record
//   A's picture under record B's id (a later Done would write A's pixels into B);
// - the answer of `shot_editor_get` against a `shot:editor` that arrives while it is in flight
//   (IPC answers and events are not ordered): the event is newer — the answer is dropped.
// Pure, node-tested (tests/editorWindow.test.mjs).

export class LoadOrder {
  private seq = 0;
  private gen = 0;

  /** A load starts: its ticket. */
  begin(): number {
    return ++this.seq;
  }

  /** Is the load with this ticket still the newest one? */
  current(ticket: number): boolean {
    return ticket === this.seq;
  }

  /** Before asking `shot_editor_get`: a mark to check its answer against. */
  mark(): number {
    return this.gen;
  }

  /** A `shot:editor` arrived. */
  switched(): void {
    this.gen++;
  }

  /** Did a `shot:editor` arrive since `mark` (the answer is older than the event)? */
  superseded(mark: number): boolean {
    return mark !== this.gen;
  }
}
