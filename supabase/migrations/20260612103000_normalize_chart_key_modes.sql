-- Drop the old keys_* constraint before rewriting mode values to *K.
alter table public.charts
  drop constraint charts_mode_known;

update public.charts
set mode = case mode
  when 'keys_2' then '2K'
  when 'keys_4' then '4K'
  when 'keys_5' then '5K'
  when 'keys_6' then '6K'
  when 'keys_7' then '7K'
  when 'keys_8' then '8K'
  when 'keys_9' then '9K'
  when 'keys_10' then '10K'
  when 'keys_14' then '14K'
  when 'keys_18' then '18K'
  when 'keys_22' then '22K'
  when 'keys_24' then '24K'
  when 'keys_48' then '48K'
  else mode
end
where mode like 'keys\_%' escape '\';

update public.scores
set key_mode = case key_mode
  when 'keys_2' then '2K'
  when 'keys_4' then '4K'
  when 'keys_5' then '5K'
  when 'keys_6' then '6K'
  when 'keys_7' then '7K'
  when 'keys_8' then '8K'
  when 'keys_9' then '9K'
  when 'keys_10' then '10K'
  when 'keys_14' then '14K'
  when 'keys_18' then '18K'
  when 'keys_22' then '22K'
  when 'keys_24' then '24K'
  when 'keys_48' then '48K'
  else key_mode
end
where key_mode like 'keys\_%' escape '\';

alter table public.charts
  add constraint charts_mode_known check (
    mode in (
      '2K',
      '4K',
      '5K',
      '6K',
      '7K',
      '8K',
      '9K',
      '10K',
      '14K',
      '18K',
      '22K',
      '24K',
      '48K',
      'unknown'
    )
  );
