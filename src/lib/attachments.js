export function parseAttachments(email) {
  if (!email?.attachments_json) return [];
  try {
    const parsed = JSON.parse(email.attachments_json);
    return Array.isArray(parsed) ? parsed.filter((a) => a?.attachment_id) : [];
  } catch {
    return [];
  }
}

export function hasAttachments(email) {
  const raw = email?.has_attachments;
  if (raw === true || raw === 1 || String(raw).toLowerCase() === "true" || raw === "1") return true;
  return parseAttachments(email).length > 0;
}

export async function fileToAttachment(file) {
  const bytes = new Uint8Array(await file.arrayBuffer());
  let binary = "";
  for (let i = 0; i < bytes.length; i += 0x8000) {
    binary += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  }
  return {
    filename: file.name,
    contentType: file.type || "application/octet-stream",
    dataBase64: btoa(binary),
  };
}
