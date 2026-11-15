<script lang="ts">
  /**
   * "Remove StarOrg changes" (spec 7.6), shared by Home and Backups. When the loose file in place
   * belongs to another tool, the dialog says so, the user must tick that it may go, and the
   * button is destructive; the request then carries `allow_foreign` (M1 `remove(.., yes)`).
   */
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
  import * as Alert from '@/components/ui/alert';
  import ActionDialog from './ActionDialog.svelte';
  import { api } from '@/lib/contract/client';
  import type { ChannelSummary } from '@/lib/contract/types';
  import { app } from '@/lib/stores/app.svelte';

  let { open = $bindable(false), channel, ondone }: { open: boolean; channel: ChannelSummary; ondone?: () => void } = $props();
  const foreign = $derived(channel.foreign_file);
  const tool = $derived(channel.foreign_tool ?? 'another tool');

  async function run() {
    const target = app.target;
    if (!target) return;
    await app.write('remove', () => api.removeChanges({ target, allow_foreign: foreign }), { inline: true });
    ondone?.();
  }
</script>

<ActionDialog
  bind:open
  title={`Remove StarOrg changes from ${channel.channel}?`}
  description={foreign ? `This deletes ${tool}'s text file from ${channel.channel}; the game goes back to its stock text.` : `The game goes back to its stock text on ${channel.channel}.`}
  confirmLabel={foreign ? `Remove ${tool}'s file` : 'Remove changes'}
  busyLabel="Removing…"
  destructive={foreign}
  ack={foreign ? `I understand that ${tool}'s file will be removed. A backup is kept, so it can be restored.` : null}
  disabledReason={channel.game_running ? `Game running: close it to remove changes on ${channel.channel}.` : null}
  onconfirm={run}
>
  {#if foreign}
    <Alert.Root variant="warning">
      <TriangleAlert aria-hidden="true" />
      <Alert.Title>A file from {tool} is in place</Alert.Title>
      <Alert.Description>It is backed up first. Run only one text tool per channel, or each will undo the other.</Alert.Description>
    </Alert.Root>
  {:else}
    <p>Your settings and edits are kept; apply again any time.</p>
  {/if}
</ActionDialog>
