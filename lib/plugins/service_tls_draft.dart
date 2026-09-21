import 'service_run_control.dart';

/// Local picker draft retained by its owning backend session, never persisted.
class ServiceTlsDraft {
  String certificate = '', privateKey = '';
  ServiceTlsSelection? checked;
  bool accepted = false;
}
