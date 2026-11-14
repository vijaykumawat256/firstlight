import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, errorMessage, type AppSettings } from "@/api";
import { useToasts } from "@/design";

const KEY = ["settings"] as const;

/** Settings with autosave: every change is applied at once, and put back with a message if it is refused. */
export function useSettings() {
  const qc = useQueryClient();
  const push = useToasts((s) => s.push);
  const query = useQuery({ queryKey: KEY, queryFn: () => api.getSettings(), staleTime: Infinity });

  const save = useMutation({
    mutationFn: (next: AppSettings) => api.setSettings(next),
    onMutate: async (next) => {
      await qc.cancelQueries({ queryKey: KEY });
      const previous = qc.getQueryData<AppSettings>(KEY);
      qc.setQueryData(KEY, next); // optimistic: the control moves immediately
      return { previous };
    },
    onError: (e, _next, ctx) => {
      if (ctx?.previous) qc.setQueryData(KEY, ctx.previous);
      push({ title: "Couldn't save that setting", description: errorMessage(e), tone: "danger" });
    },
    onSuccess: (saved) => {
      qc.setQueryData(KEY, saved); // the backend may have corrected a value
      void qc.invalidateQueries({ queryKey: ["briefing"] });
    },
  });

  /** Edit a copy of the current settings. */
  const update = (edit: (draft: AppSettings) => void) => {
    const current = qc.getQueryData<AppSettings>(KEY);
    if (!current) return;
    const next = structuredClone(current);
    edit(next);
    save.mutate(next);
  };

  return { settings: query.data, isPending: query.isPending, update };
}
