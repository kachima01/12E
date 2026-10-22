-- Run this in the Supabase SQL editor before deploying the app.
create extension if not exists pgcrypto;

create table if not exists public.profiles (
  id uuid primary key references auth.users (id) on delete cascade,
  full_name text not null check (char_length(full_name) between 2 and 80),
  role text not null default 'member' check (role in ('member', 'admin')),
  status text not null default 'approved' check (status in ('pending', 'approved', 'rejected')),
  created_at timestamptz not null default now()
);

create table if not exists public.contributions (
  id uuid primary key default gen_random_uuid(),
  user_id uuid not null references public.profiles (id) on delete restrict,
  amount numeric(12, 2) not null check (amount > 0),
  currency text not null default 'ZMW' check (currency = 'ZMW'),
  payment_reference text not null unique,
  status text not null default 'pending' check (status in ('pending', 'confirmed', 'rejected')),
  method text not null default 'mobile_money' check (method = 'mobile_money'),
  created_at timestamptz not null default now(),
  confirmed_at timestamptz,
  receipt_sent_at timestamptz
);

create table if not exists public.contribution_payment_details (
  contribution_id uuid primary key references public.contributions (id) on delete cascade,
  payer_phone text not null check (payer_phone ~ '^\+260[79][0-9]{8}$'),
  provider_reference text,
  created_at timestamptz not null default now()
);

alter table public.contribution_payment_details
  add column if not exists provider_reference text;
create unique index if not exists contribution_payment_provider_reference_idx
  on public.contribution_payment_details (provider_reference)
  where provider_reference is not null;

create table if not exists public.lipila_webhook_events (
  webhook_id text primary key,
  event_type text not null,
  processed_at timestamptz not null default now()
);

create table if not exists public.class_messages (
  id uuid primary key default gen_random_uuid(),
  user_id uuid not null references public.profiles (id) on delete cascade,
  message text not null check (char_length(btrim(message)) between 1 and 500),
  created_at timestamptz not null default now()
);

-- Move payment numbers from an earlier contribution-column version into the
-- private table, then remove them from the member-visible table.
do $$
begin
  if exists (
    select 1 from information_schema.columns
    where table_schema = 'public'
      and table_name = 'contributions'
      and column_name = 'payer_phone'
  ) then
    insert into public.contribution_payment_details (contribution_id, payer_phone)
    select id, payer_phone
    from public.contributions
    where payer_phone is not null
    on conflict (contribution_id) do nothing;
    alter table public.contributions drop column payer_phone;
  end if;
end;
$$;

create index if not exists contributions_user_created_idx
  on public.contributions (user_id, created_at desc);
create index if not exists contributions_status_created_idx
  on public.contributions (status, created_at desc);

create or replace function public.create_profile_for_new_member()
returns trigger
language plpgsql
security definer
set search_path = ''
as $$
begin
  insert into public.profiles (id, full_name, status)
  values (new.id, coalesce(new.raw_user_meta_data ->> 'full_name', 'New member'), 'approved')
  on conflict (id) do update set status = 'approved';
  return new;
end;
$$;

drop trigger if exists on_auth_user_created on auth.users;
create trigger on_auth_user_created
  after insert on auth.users
  for each row execute procedure public.create_profile_for_new_member();

create or replace function public.is_approved_admin()
returns boolean
language sql
stable
security definer
set search_path = ''
as $$
  select exists (
    select 1 from public.profiles
    where id = (select auth.uid()) and role = 'admin' and status = 'approved'
  );
$$;

create or replace function public.is_approved_member()
returns boolean
language sql
stable
security definer
set search_path = ''
as $$
  select exists (
    select 1 from public.profiles
    where id = (select auth.uid()) and status = 'approved'
  );
$$;

create or replace function public.protect_profile_privileges()
returns trigger
language plpgsql
security definer
set search_path = ''
as $$
begin
  if (new.role is distinct from old.role or new.status is distinct from old.status)
     and not public.is_approved_admin() then
    raise exception 'Only an approved admin can change member role or status.';
  end if;
  return new;
end;
$$;

drop trigger if exists protect_profile_privileges on public.profiles;
create trigger protect_profile_privileges
  before update on public.profiles
  for each row execute procedure public.protect_profile_privileges();

alter table public.profiles enable row level security;
alter table public.contributions enable row level security;
alter table public.contribution_payment_details enable row level security;
alter table public.lipila_webhook_events enable row level security;
alter table public.class_messages enable row level security;
revoke all on public.contribution_payment_details, public.lipila_webhook_events from anon, authenticated;
revoke all on public.class_messages from anon, authenticated;

drop policy if exists "Members can read approved profiles" on public.profiles;
create policy "Members can read approved profiles"
  on public.profiles for select to authenticated
  using (
    public.is_approved_admin()
    or (status = 'approved' and public.is_approved_member())
    or id = (select auth.uid())
  );

drop policy if exists "Admins can update membership status" on public.profiles;
create policy "Admins can update membership status"
  on public.profiles for update to authenticated
  using (public.is_approved_admin())
  with check (role = 'member' or public.is_approved_admin());

drop policy if exists "Members can update their own profile name" on public.profiles;
create policy "Members can update their own profile name"
  on public.profiles for update to authenticated
  using (id = (select auth.uid()))
  with check (id = (select auth.uid()));

drop policy if exists "Approved members can read confirmed contributions" on public.contributions;
create policy "Approved members can read confirmed contributions"
  on public.contributions for select to authenticated
  using (
    public.is_approved_admin()
    or (public.is_approved_member() and status = 'confirmed')
    or user_id = (select auth.uid())
  );

drop policy if exists "Admins can update contributions" on public.contributions;
create policy "Admins can update contributions"
  on public.contributions for update to authenticated
  using (public.is_approved_admin())
  with check (public.is_approved_admin());

drop policy if exists "Approved members can read class messages" on public.class_messages;
create policy "Approved members can read class messages"
  on public.class_messages for select to authenticated
  using (public.is_approved_member());

drop policy if exists "Approved members can post their own messages" on public.class_messages;
create policy "Approved members can post their own messages"
  on public.class_messages for insert to authenticated
  with check (user_id = (select auth.uid()) and public.is_approved_member());

grant usage on schema public to authenticated;
grant select on public.profiles to authenticated;
revoke update on public.profiles from authenticated;
grant update (full_name, status) on public.profiles to authenticated;
grant select, insert on public.class_messages to authenticated;
revoke select, update on public.contributions from authenticated;
grant select (id, user_id, amount, currency, payment_reference, status, method, created_at, confirmed_at)
  on public.contributions to authenticated;
grant update (status, confirmed_at) on public.contributions to authenticated;

do $$
begin
  if not exists (
    select 1 from pg_publication_tables
    where pubname = 'supabase_realtime' and schemaname = 'public' and tablename = 'contributions'
  ) then
    alter publication supabase_realtime add table public.contributions;
  end if;
  if not exists (
    select 1 from pg_publication_tables
    where pubname = 'supabase_realtime' and schemaname = 'public' and tablename = 'profiles'
  ) then
    alter publication supabase_realtime add table public.profiles;
  end if;
  if not exists (
    select 1 from pg_publication_tables
    where pubname = 'supabase_realtime' and schemaname = 'public' and tablename = 'class_messages'
  ) then
    alter publication supabase_realtime add table public.class_messages;
  end if;
end;
$$;

-- After creating your account and verifying its email, run this once in SQL Editor:
-- update public.profiles set role = 'admin', status = 'approved'
-- where id = (select id from auth.users where email = 'YOUR_ADMIN_EMAIL');
