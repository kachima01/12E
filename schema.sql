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

alter table public.profiles enable row level security;
alter table public.contributions enable row level security;

create policy "Members can read approved profiles"
  on public.profiles for select to authenticated
  using (
    public.is_approved_admin()
    or (status = 'approved' and public.is_approved_member())
    or id = (select auth.uid())
  );

create policy "Admins can update membership status"
  on public.profiles for update to authenticated
  using (public.is_approved_admin())
  with check (role = 'member' or public.is_approved_admin());

create policy "Approved members can read confirmed contributions"
  on public.contributions for select to authenticated
  using (
    public.is_approved_admin()
    or (public.is_approved_member() and status = 'confirmed')
    or user_id = (select auth.uid())
  );

create policy "Admins can update contributions"
  on public.contributions for update to authenticated
  using (public.is_approved_admin())
  with check (public.is_approved_admin());

grant usage on schema public to authenticated;
grant select, update on public.profiles to authenticated;
grant select, update on public.contributions to authenticated;

-- After creating your account and verifying its email, run this once in SQL Editor:
-- update public.profiles set role = 'admin', status = 'approved'
-- where id = (select id from auth.users where email = 'YOUR_ADMIN_EMAIL');
