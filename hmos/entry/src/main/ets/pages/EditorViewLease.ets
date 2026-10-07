// This is an application admission boundary. aboutToDisappear runs before
// destruction; it does not assert that the SDK event queue has been drained.
@Component
export struct EditorViewLease {
  @Prop owner: string = '';
  @BuilderParam content: (owner: string) => void;
  onMounted: (owner: string) => void = (_owner: string): void => {};
  onRevoked: (owner: string) => void = (_owner: string): void => {};
  @State private mountedOwner: string = '';

  aboutToAppear(): void {
    this.mountedOwner = this.owner;
    this.onMounted(this.mountedOwner);
  }
  aboutToDisappear(): void {
    const owner = this.mountedOwner;
    this.mountedOwner = '';
    if (owner) { this.onRevoked(owner); }
  }
  build() {
    if (this.mountedOwner) {
      this.content(this.mountedOwner)
    }
  }
}
