<script>
  import { startDrag, toggleMaximize } from "../../lib/windowDrag.js";
  import WindowControls from "./WindowControls.svelte";
</script>

<header class="app-header" onpointerdown={startDrag} ondblclick={toggleMaximize}>
  <div class="app-header-controls">
    <WindowControls />
  </div>
</header>
