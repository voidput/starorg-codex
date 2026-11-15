<script lang="ts">
  /** A failed load: what failed, why, and a retry. Errors are Alerts (spec 9.5). */
  import CircleAlert from '@lucide/svelte/icons/circle-alert';
  import RefreshCw from '@lucide/svelte/icons/refresh-cw';
  import * as Alert from '@/components/ui/alert';
  import { Button } from '@/components/ui/button';

  let { title, error, onretry }: { title: string; error: { message: string; detail?: string | null }; onretry?: () => void } = $props();
</script>

<Alert.Root variant="destructive" role="alert">
  <CircleAlert aria-hidden="true" />
  <Alert.Title>{title}</Alert.Title>
  <Alert.Description>
    <p>{error.message}</p>
    {#if error.detail}<p class="mono mt-1 break-all">{error.detail}</p>{/if}
    {#if onretry}
      <Button variant="outline" size="sm" class="mt-2" onclick={onretry}><RefreshCw aria-hidden="true" /> Try again</Button>
    {/if}
  </Alert.Description>
</Alert.Root>
