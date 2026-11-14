import { create } from "zustand";

interface SearchState {
  open: boolean;
  setOpen: (open: boolean) => void;
}

/** Whether the search palette is showing. Any screen can open it; ⌘K / Ctrl+K toggles it everywhere. */
export const useSearch = create<SearchState>((set) => ({
  open: false,
  setOpen: (open) => set({ open }),
}));
