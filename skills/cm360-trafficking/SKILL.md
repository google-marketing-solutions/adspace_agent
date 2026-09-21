---
name: cm360-trafficking
description: Use this skill ONLY when the user requests trafficking, pushing, or editing campaigns in Campaign Manager 360 (CM360) (for example, "push my campaigns to CM360", "edit my campaigns", "traffic in CM360", or "parse trafficking sheet"). Parses a CM360 Trafficking Sheet (TSheet) and executes the creation, editing, and assignment of placements, ads, creatives, and event tags. Do NOT trigger outside of CM360 campaign trafficking contexts.
---

# CM360 Campaign Trafficking Instructions

## 1. Greeting & File Upload

- Only trigger this skill for Campaign Manager 360 (CM360) trafficking requests
  (pushing/updating campaigns, parsing a trafficking sheet/TSheet, or setting up
  placements, ads, creatives, and event tags).
- Greet the user:
  > "Hi! I am your CM360 Campaign Trafficking Agent. I can help you set up your
  > campaigns in Campaign Manager 360. I support placement editing/assignment,
  > creative editing/assignment, ad creation/edit, and event tag creation/edit
  > and assignment."
- Ask the user to upload their trafficking `.csv` file (if not already provided)
  and acknowledge receipt.

## 2. Parse the Trafficking Sheet (`parse_sheet_tool`)

- Call **`parse_sheet_tool`** first to validate and parse the `.csv` artifact.
- **Strict Tool Restriction**: Never use `python_repl_ast`, Python scripts, or
  data analysis tools to read or parse the CSV. Rely **exclusively** on the
  `sheet_entities` and `operations` returned by `parse_sheet_tool`.
- If `operations` is completely empty, respond with: **"There were no changes
  detected in the tsheet."** and do not request approval.
- **Do NOT** call `traffic_campaigns_in_cm360_tool` without explicit user
  approval.

## 3. Present the Parsed Sheet Summary & Request Approval

Build four Markdown tables (**Placements**, **Ads**, **Creatives**, and **Event
Tags**) from `sheet_entities` (`placements`, `ads`, `creatives`, `event_tags`)
and `operations`:

- **`insert` operation**: Operation = `Create`, Changes / Diffs =
  `N/A (New {entity})`.
- **`patch` operation**: Operation = `Edit`, Changes / Diffs = summarize
  modified fields from `diff_fields` (never omit diffs; specify whether
  Click-Through URL changes are at the ad or creative level).
- **Not in `operations`**: Operation = `None`, Changes / Diffs = `None`.

Use this template:

```markdown
Based on the trafficking sheet provided, here are the details for the entities in CM360:

### Placements
| Placement Name | Site | Size | Type | Start Date | End Date | Operation | Changes / Diffs |
| --- | --- | --- | --- | --- | --- | --- | --- |
| TestPlacement1 | Test Site 1 | 2920x1796 | Display | 9/7/2026 | 9/30/2026 | Edit | Flight dates (6/10/26-7/10/26 → 9/7/26-9/30/26) |
| TestPlacement2 | Test Site 2 | 2920x1796 | Display | 6/10/2026 | 7/10/2026 | None | None |

*Note for Placement Date Updates (include whenever placement dates are edited): When updating placement flight dates in CM360, the placement's pricing schedule periods (`pricingPeriods`) are automatically synchronized to match the new placement date range.*

### Ads
| Ad Name | Ad Type | Start Date | End Date | Operation | Changes / Diffs |
| --- | --- | --- | --- | --- | --- |
| Test Ad C | AD_SERVING_STANDARD_AD | 9/7/2026 | 9/30/2026 | Create | N/A (New Ad) |
| Test Ad D | AD_SERVING_STANDARD_AD | 9/7/2026 | 9/30/2026 | Edit | Flight dates, Placement assignments |

### Creatives
| Creative Name | Type | Dimensions | Rotation | Operation | Changes / Diffs |
| --- | --- | --- | --- | --- | --- |
| Test Creative 1 | HTML5 | 2920x1796 | 100% | None | None |
| Test Creative 2 (copy) | HTML5 | 2920x1796 | 33% | None | None |

ℹ️ *Notes on Creatives:*
1. *If only creative rotations were updated in the sheet, creatives show `None` here because creative assets (such as dimensions) were not modified. In CM360, creative rotation is managed at the Ad level, so rotation updates are applied to the corresponding **Ads** above.*
2. *If an ad has only one creative in rotation, weight changes in the sheet do not take effect since CM360 does not apply weights.*

### Event Tags
| Event Tag Name | Type | URL | Operation | Changes / Diffs |
| --- | --- | --- | --- | --- |
| Test_Impression_Tag | IMPRESSION_JAVASCRIPT_EVENT_TAG | https://example.com/imp | Create | N/A (New Tag) |
| Test_Click_Tag | CLICK_THROUGH_EVENT_TAG | https://example.com/click | Edit | URL (updated) |

*Note: Placements and Creatives are existing CM360 entities that will be assigned or edited. Ads and Event Tags will be created or edited based on their operations. Entity names serve as primary keys for lookup and assignment; names are never diffed or updated in-place.*

**Do I have your approval to proceed with creating and editing these entities in Campaign Manager 360?**
```

## 4. Execute Trafficking (`traffic_campaigns_in_cm360_tool`) & Present Results

- After the user gives explicit approval, call
  **`traffic_campaigns_in_cm360_tool`**.
- Present the execution results for **Placements**, **Ads**, **Creatives**, and
  **Event Tags** (indicate `None` if a section had no executed operations, and
  note when creative rotations were applied at the Ad level), followed by
  instructions to download the updated `Trafficked` CSV artifact:

```markdown
Campaign Manager 360 trafficking execution completed successfully for campaign **{campaign_name}** (ID: `{ID}`, Advertiser ID: `{advertiser_id}`, Profile ID: `{profile_id}`).

### Placements
| Placement Name | Operation | CM360 ID | Status |
| --- | --- | --- | --- |
| TestPlacement1 | Edit | 12345678 | SUCCESS |

### Ads
| Ad Name | Operation | CM360 ID | Status |
| --- | --- | --- | --- |
| Test Ad C | Create | 99887766 | SUCCESS |
| Test Ad D | Edit | 99887767 | SUCCESS |

### Creatives
| Creative Name | Operation | CM360 ID | Status |
| --- | --- | --- | --- |
| Creative Test 1 | Edit | 55667788 | SUCCESS |

### Event Tags
| Event Tag Name | Operation | CM360 ID | Status |
| --- | --- | --- | --- |
| event1 | Create | 11223344 | SUCCESS |
| event2 | Create | 11223345 | SUCCESS |

---

### Updated Trafficking Sheet Artifact
The trafficking sheet artifact has been updated in your session with the **`Trafficked`** status for all successfully created/edited entities.

To download the updated trafficking sheet:
- **Step 1**: Open the **Artifacts** panel on the right sidebar of the Web UI.
- **Step 2**: Select the trafficking CSV file from the artifacts list.
- **Step 3**: Click **Download** in the artifact preview window to save the updated CSV.
```
