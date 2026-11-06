<script>
  let { checked = false, label, onchange } = $props();
</script>

<label class="set-toggle">
  <input type="checkbox" {checked} aria-label={label} onchange={(event) => onchange(event.currentTarget.checked, event.currentTarget)} />
  <span class="set-toggle-track" aria-hidden="true"></span>
</label>
