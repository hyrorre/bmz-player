ALTER TABLE charts ADD COLUMN hln_notes integer NOT NULL DEFAULT 0;
--> statement-breakpoint
ALTER TABLE charts ADD COLUMN has_defined_hln integer NOT NULL DEFAULT 0;
--> statement-breakpoint
ALTER TABLE charts ADD COLUMN has_hln integer NOT NULL DEFAULT 0;
--> statement-breakpoint
ALTER TABLE charts ADD COLUMN has_conditional integer NOT NULL DEFAULT 0;
--> statement-breakpoint
ALTER TABLE charts ADD COLUMN kaleid text NOT NULL DEFAULT '{}';
