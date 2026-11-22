import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { api, errorMessage, type SignInApps, type SignInAppsChange } from "@/api";
import { isPhone } from "@/app/platform";
import { Button, Dialog, TextField, useToasts } from "@/design";

const FORM_ID = "sign-in-apps-form";

/**
 * Your own Google / Microsoft sign-in app, set here instead of being built into the app. Needed only when a provider
 * says "Set up" (this copy of Firstlight has no app of its own for it) or when a company requires its own registered
 * app. Kept on this device only and never in a settings export; the Google secret goes in the system keychain.
 */
export function SignInAppsDialog({ open, onOpenChange, focus }: { open: boolean; onOpenChange: (o: boolean) => void; focus?: "google" | "microsoft" }) {
  const current = useQuery({ queryKey: ["sign-in-apps"], queryFn: () => api.getSignInApps(), enabled: open });
  return (
    <Dialog
      open={open}
      onOpenChange={onOpenChange}
      title="Your own sign-in app"
      description="Google and Microsoft only let registered apps sign in. Firstlight normally brings its own; put yours here if it doesn't, or if your company requires its own."
      footer={
        <>
          <Button onClick={() => onOpenChange(false)}>Cancel</Button>
          <Button type="submit" form={FORM_ID} variant="primary" disabled={!current.data}>
            Save
          </Button>
        </>
      }
    >
      {current.data ? <Form key={JSON.stringify(current.data)} apps={current.data} focus={focus} onDone={() => onOpenChange(false)} /> : <p className="text-body text-fg-2">Loading…</p>}
    </Dialog>
  );
}

function Form({ apps, focus, onDone }: { apps: SignInApps; focus?: "google" | "microsoft"; onDone: () => void }) {
  const qc = useQueryClient();
  const push = useToasts((t) => t.push);
  const [google, setGoogle] = useState(apps.google_client_id ?? "");
  const [secret, setSecret] = useState("");
  const [microsoft, setMicrosoft] = useState(apps.microsoft_client_id ?? "");
  const save = useMutation({
    mutationFn: (change: SignInAppsChange) => api.setSignInApps(change),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["sign-in-apps"] });
      void qc.invalidateQueries({ queryKey: ["oauth-available"] });
      push({ title: "Sign-in app saved", description: "Accounts you connected before may ask you to sign in again." });
      onDone();
    },
  });
  const change: SignInAppsChange = {
    google_client_id: google.trim(),
    microsoft_client_id: microsoft.trim(),
    ...(secret.trim() ? { google_client_secret: secret.trim() } : {}),
  };
  const fallback = (builtIn: boolean) => (builtIn ? "Leave empty to use the one built into Firstlight." : "This copy of Firstlight has none built in, so sign-in needs one.");

  return (
    <form
      id={FORM_ID}
      className="flex flex-col gap-4"
      onSubmit={(e) => {
        e.preventDefault();
        if (!save.isPending) save.mutate(change);
      }}
    >
      <fieldset className="flex flex-col gap-3">
        <legend className="mb-1 text-headline font-semibold text-fg">Google</legend>
        <TextField
          label="Client ID"
          value={google}
          onChange={(e) => setGoogle(e.target.value)}
          placeholder="1234…apps.googleusercontent.com"
          autoFocus={!isPhone && focus !== "microsoft"}
          spellCheck={false}
          autoComplete="off"
          hint={fallback(apps.built_in_google)}
        />
        <TextField
          label="Client secret"
          type="password"
          value={secret}
          onChange={(e) => setSecret(e.target.value)}
          placeholder={apps.google_has_secret ? "Saved (type to replace)" : "GOCSPX-…"}
          autoComplete="off"
          hint="Kept in your system keychain."
        />
        <details className="text-footnote text-fg-2">
          <summary className="cursor-pointer text-accent">How to make one</summary>
          <ol className="mt-1 list-decimal space-y-1 pl-5">
            <li>console.cloud.google.com: make a project, enable the Google Calendar API.</li>
            <li>OAuth consent screen: External; add yourself as a test user.</li>
            <li>Credentials, Create credentials, OAuth client ID, Desktop app. Copy the ID and secret here.</li>
          </ol>
        </details>
      </fieldset>

      <fieldset className="flex flex-col gap-3">
        <legend className="mb-1 text-headline font-semibold text-fg">Microsoft</legend>
        <TextField
          label="Application (client) ID"
          value={microsoft}
          onChange={(e) => setMicrosoft(e.target.value)}
          placeholder="1a2b3c4d-1234-5678-9abc-1a2b3c4d5e6f"
          autoFocus={!isPhone && focus === "microsoft"}
          spellCheck={false}
          autoComplete="off"
          hint={fallback(apps.built_in_microsoft)}
        />
        <details className="text-footnote text-fg-2">
          <summary className="cursor-pointer text-accent">How to make one</summary>
          <ol className="mt-1 list-decimal space-y-1 pl-5">
            <li>entra.microsoft.com: App registrations, New registration; any organisation and personal accounts.</li>
            <li>Redirect URI: Mobile and desktop applications, http://localhost.</li>
            <li>Authentication: allow public client flows. API permissions: Calendars.Read. Copy the Application ID here.</li>
          </ol>
        </details>
      </fieldset>

      {save.isError && (
        <p role="alert" className="text-footnote text-danger">
          {errorMessage(save.error)}
        </p>
      )}
    </form>
  );
}
