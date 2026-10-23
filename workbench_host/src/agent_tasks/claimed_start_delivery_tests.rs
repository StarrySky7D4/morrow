// Scoped ordinary-owner channel regressions. No native backend or OS child.
mod claimed_start_delivery_tests {
    use super::*;
    use morrow_agent_session_exec_v1_r2::Error as SessionError;
    use crate::agent_tasks::commands::{AgentCommandErrorClass,AgentCommandStage};

    fn pair(worker:&AgentWorker<OrdinaryOwner>)->(Command,AgentCommandHandle) {
        let (command,handle)=AgentCommandHandle::pair(AgentCommand::OpenNativeSession,worker.control.clone());
        command.started.store(true,Ordering::Release);
        (command,handle)
    }
    fn deny(command:&Command)->Result<AgentReply,AgentError> {
        complete_claimed_start::<()>(Err(SessionError::Denied),&command.diagnostic,|_|panic!("denial registered a provider"))
    }
    fn finish(mut worker:AgentWorker<OrdinaryOwner>) {
        worker.stop();let exit=join(&mut worker);
        assert!(exit.disconnect.is_ok()&&exit.maintenance.is_ok()&&exit.cleanup.is_none());
    }
    #[test]
    fn original_denial_is_delivered_once_on_the_original_channel() {
        let _serial=SERIAL.lock().unwrap_or_else(|e|e.into_inner());
        let (owner,package,context)=fixture_with_process(false);assert!(context.port().is_none());
        let worker=spawn(owner,package,context);let (command,mut handle)=pair(&worker);
        deliver_command_result(&command.reply,&command.diagnostic,&worker.control,deny(&command));
        assert!(matches!(handle.try_read(),Err(AgentError::Session(SessionError::Denied))));
        assert!(matches!(handle.try_read(),Err(AgentError::Invalid)));
        assert!(worker.control.live());
        let observed=handle.diagnostic_snapshot();
        assert_eq!(observed.worker_stage,AgentCommandStage::PortRejected);
        assert_eq!(observed.delivery_stage,AgentCommandStage::ReceiverError);
        assert_eq!(observed.delivery_error_class,AgentCommandErrorClass::R2Denied);
        assert!(!observed.provider_observed);
        drop(command);drop(handle);finish(worker);
    }
    #[test]
    fn uncertain_port_error_is_delivered_as_unknown_and_stops_original_control() {
        let _serial=SERIAL.lock().unwrap_or_else(|e|e.into_inner());
        for error in [SessionError::CommitUnknown,SessionError::Storage,SessionError::Correlation] {
            let (owner,package,context)=fixture_with_process(false);let worker=spawn(owner,package,context);
            let (command,mut handle)=pair(&worker);
            let result=complete_claimed_start::<()>(Err(error),&command.diagnostic,|_|panic!("error registered a provider"));
            deliver_command_result(&command.reply,&command.diagnostic,&worker.control,result);
            assert!(matches!(handle.try_read(),Err(AgentError::Unknown)));
            assert!(!worker.control.live());drop(command);drop(handle);finish(worker);
        }
    }
    #[test]
    fn failure_after_observed_provider_stays_unknown_even_if_registration_is_unavailable() {
        let _serial=SERIAL.lock().unwrap_or_else(|e|e.into_inner());
        let (owner,package,context)=fixture_with_process(false);let worker=spawn(owner,package,context);
        let (command,mut handle)=pair(&worker);
        let original=Box::new(7u32);let pointer=&*original as *const u32;
        let result=complete_claimed_start(Ok(original),&command.diagnostic,|provider| {
            assert_eq!(&*provider as *const u32,pointer);
            Err(AgentError::Unavailable)
        });
        deliver_command_result(&command.reply,&command.diagnostic,&worker.control,result);
        assert!(matches!(handle.try_read(),Err(AgentError::Unknown)));
        assert!(!worker.control.live());drop(command);drop(handle);finish(worker);
    }
    #[test]
    fn cancelled_before_delivery_does_not_publish_a_clean_denial() {
        let _serial=SERIAL.lock().unwrap_or_else(|e|e.into_inner());
        let (owner,package,context)=fixture_with_process(false);let worker=spawn(owner,package,context);
        let (command,mut handle)=pair(&worker);let result=deny(&command);handle.cancel();
        deliver_command_result(&command.reply,&command.diagnostic,&worker.control,result);
        assert!(matches!(handle.try_read(),Err(AgentError::Unknown)));
        drop(command);drop(handle);finish(worker);
    }
    #[test]
    fn cancellation_after_send_vetoes_denial_for_both_original_receive_paths() {
        let _serial=SERIAL.lock().unwrap_or_else(|e|e.into_inner());
        for blocking in [false,true] {
            let (owner,package,context)=fixture_with_process(false);let worker=spawn(owner,package,context);
            let (command,mut handle)=pair(&worker);
            deliver_command_result(&command.reply,&command.diagnostic,&worker.control,deny(&command));
            handle.cancel();
            if blocking {assert!(matches!(handle.wait(),Err(AgentError::Unknown)));}
            else {assert!(matches!(handle.try_read(),Err(AgentError::Unknown)));drop(handle);}
            drop(command);finish(worker);
        }
    }
    #[test]
    fn lost_original_receiver_stops_control_without_delivery_substitution() {
        let _serial=SERIAL.lock().unwrap_or_else(|e|e.into_inner());
        let (owner,package,context)=fixture_with_process(false);let worker=spawn(owner,package,context);
        let (command,handle)=pair(&worker);let result=deny(&command);drop(handle);
        deliver_command_result(&command.reply,&command.diagnostic,&worker.control,result);
        assert!(!worker.control.live());
        assert_eq!(command.diagnostic.snapshot().delivery_stage,AgentCommandStage::ReplySendFailed);
        drop(command);finish(worker);
    }
    #[test]
    fn dropped_original_sender_remains_unknown_despite_denial_diagnostic() {
        let _serial=SERIAL.lock().unwrap_or_else(|e|e.into_inner());
        let (owner,package,context)=fixture_with_process(false);let worker=spawn(owner,package,context);
        let (command,mut handle)=pair(&worker);let result=deny(&command);
        assert!(matches!(result,Err(AgentError::Session(SessionError::Denied))));
        drop(command);
        assert!(matches!(handle.try_read(),Err(AgentError::Unknown)));
        assert_eq!(handle.diagnostic_snapshot().delivery_stage,AgentCommandStage::ReceiverDisconnected);
        drop(handle);finish(worker);
    }
}
