# Wraps an already-built tree; rpmbuild is not asked to compile anything.
# build-linux.sh stages the files and passes their location as _shotr_stage,
# because rpmbuild empties %{buildroot} at the start of %install — staging
# straight into it looks like it should work and silently produces an empty
# package.
#
# Requires: is deliberately absent. rpmbuild reads the ELF and writes it, which
# is the only version that stays true: the list gained libpipewire the day xcap
# started linking it, and nobody would have remembered to edit it here.

%global debug_package %{nil}
%define _build_id_links none

Name:           shotr
Version:        %{_shotr_version}
Release:        1
Summary:        Capture a screenshot and make it presentable
License:        GPL-3.0-only
URL:            https://github.com/thangho98/shotr

%description
Take a screenshot and make it presentable: drop it on a gradient, round the
corners, add a shadow, annotate it, redact anything sensitive, export.

Runs entirely on the machine; no image ever leaves it. The interface is
available in English and Vietnamese.

%install
mkdir -p %{buildroot}
cp -a %{_shotr_stage}/. %{buildroot}/

%files
%{_bindir}/shotr
%{_datadir}/applications/shotr.desktop
%{_datadir}/icons/hicolor/*/apps/shotr.png
