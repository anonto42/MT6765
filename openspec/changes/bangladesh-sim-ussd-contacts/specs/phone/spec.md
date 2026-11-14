# Spec: Phone USSD & SIM Contacts

## Requirements

### Requirement: USSD code execution
The phone service SHALL accept USSD codes (strings starting with `*` or `#` and ending with `#`) in `phone_dial` and `phone_ussd`, send `AT+CUSD=1,"<code>",15` to the modem with a bounded timeout, and parse `+CUSD: <m>,"<text>",<dcs>` replies.

#### Scenario: Dialing a Bangladesh operator USSD code
- **WHEN** the user dials `*121#` or `*566#` while the modem is registered
- **THEN** `walpad-core` queues `AT+CUSD=1,"*121#",15` instead of refusing the code and publishes the decoded USSD response text in `PhoneStatus.ussd`.

### Requirement: SIM phonebook loading
The phone service SHALL support loading contacts from the SIM card (`AT+CPBS="SM"`, `AT+CPBR=?`, `AT+CPBR=<start>,<end>`) via `phone_sim_contacts` and importing them into `ContactStore`.

#### Scenario: Loading SIM contacts
- **WHEN** `phone_sim_contacts` is invoked while the modem is ready
- **THEN** `walpad-core` queries the SIM phonebook range, reads entries in bounded batches, imports them without duplicating existing numbers, and returns the imported count.
