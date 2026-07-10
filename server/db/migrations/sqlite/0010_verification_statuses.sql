UPDATE `scores`
SET `verification` = CASE `verification`
  WHEN 'signed' THEN 'verified_play'
  WHEN 'trusted' THEN 'verified_play'
  WHEN 'invalid' THEN 'unverified'
  ELSE `verification`
END;--> statement-breakpoint
UPDATE `best_scores`
SET `verification` = CASE `verification`
  WHEN 'signed' THEN 'verified_play'
  WHEN 'trusted' THEN 'verified_play'
  WHEN 'invalid' THEN 'unverified'
  ELSE `verification`
END;--> statement-breakpoint
UPDATE `course_scores`
SET `verification` = CASE `verification`
  WHEN 'signed' THEN 'verified_play'
  WHEN 'trusted' THEN 'verified_play'
  WHEN 'invalid' THEN 'unverified'
  ELSE `verification`
END;--> statement-breakpoint
UPDATE `best_course_scores`
SET `verification` = CASE `verification`
  WHEN 'signed' THEN 'verified_play'
  WHEN 'trusted' THEN 'verified_play'
  WHEN 'invalid' THEN 'unverified'
  ELSE `verification`
END;
