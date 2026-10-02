<script>
  import { findVerificationCode } from "../../lib/verificationCode.js";
  import Attachments from "./Attachments.svelte";
  import CodeCard from "./CodeCard.svelte";
  import EmailBody from "./EmailBody.svelte";

  let { email } = $props();

  const code = $derived(
    findVerificationCode({ subject: email.subject, snippet: email.snippet, body: email.body_html }),
  );
</script>

{#if code}
  <CodeCard {code} />
{/if}
<Attachments {email} />
<EmailBody {email} class="email-body-text" />
